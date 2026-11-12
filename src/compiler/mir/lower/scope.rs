// Scope-exit release of owned locals.
//
// The compiler releases an owned pointer in exactly one place that used to
// exist: the rebind in `set x = ...`. A value that was never rebound was never
// released at all, so:
//
//     fn f: () :i64 {
//         set v = [] :vec[i64] mut     // allocated
//         return 1                     // v is still live here
//     }
//
// leaked the vector on every call, and a loop body that built a value per
// iteration leaked once per iteration. That is the mechanism behind the C2
// allocation kernel's growth, and it is a correctness problem before it is a
// performance one.
//
// Releasing at scope exit has one dangerous failure mode: freeing a value the
// caller now owns. Three things prevent that here:
//
//   1. A local that escapes — returned, or stored into a struct, global, or
//      collection — is recorded and skipped.
//   2. Only locals whose type is an owned pointer are released. Scalars, and
//      borrows of someone else's storage, are left alone.
//   3. The walk stops at the first `break`, `continue`, or `return` in the
//      body. After such a statement the remaining statements are not reachable
//      on that path, so releasing what follows would release values that were
//      never bound on that path.
//
// This is deliberately conservative. A missed release is a leak, which is what
// we had before; a wrong release is a use-after-free, which is worse.

use super::MirLower;
use crate::compiler::mir::{MirOp, MirType, MirValue};
use crate::parser::ast::DataType;

/// True when a local of this type owns the pointer it holds and must therefore
/// be released when it goes out of scope.
///
/// Mirrors `ownership_of` in `stmt.rs`: the destructor decision is made from
/// the semantic type, never from LLVM's `ptr`. `Borrowed` is the important
/// exclusion — a `&vec` points at storage someone else owns, so releasing it
/// here would free the owner's vector.
fn owns_pointer_on_scope_exit(ty: &DataType) -> bool {
    matches!(
        ty,
        DataType::Str
            | DataType::Vector { .. }
            | DataType::Map { .. }
            | DataType::Dict
            | DataType::List
    )
}

impl MirLower {
    /// Enter a new lexical scope.
    pub(crate) fn begin_scope(&mut self) {
        self.scope_stack
            .push(crate::compiler::mir::lower::ScopeFrame::default());
    }

    /// Leave the innermost scope, releasing what it owned.
    pub(crate) fn end_scope_and_release(&mut self) {
        self.release_scope_locals();
        self.scope_stack.pop();
    }

    fn frame_mut(&mut self) -> &mut crate::compiler::mir::lower::ScopeFrame {
        // A `lower_statement` always runs inside at least the function scope,
        // but codegen entry points can reach lowering without it; keep a frame
        // alive rather than panicking on an empty stack.
        if self.scope_stack.is_empty() {
            self.scope_stack
                .push(crate::compiler::mir::lower::ScopeFrame::default());
        }
        self.scope_stack.last_mut().expect("frame just ensured")
    }

    /// Record a local that owns its pointer, so scope exit can release it.
    ///
    /// Re-binding an existing name replaces the previous entry: the old value
    /// is released by the rebind itself, and the new one is what scope exit owns.
    pub(crate) fn track_owned_local(&mut self, name: &str, slot: usize, ty: DataType) {
        if !owns_pointer_on_scope_exit(&ty) {
            return;
        }
        // A name may be declared in an enclosing scope and rebound in a nested
        // one — `set v = []` in a loop body followed by `set v = push(v, x)` in
        // a loop inside it. Both are the same slot, so ownership belongs to the
        // DECLARING scope. Recording it in both frames would release it twice:
        // once when the inner scope ended and once when the outer did, and the
        // second release is a double free.
        //
        // So the search runs from innermost outward, an existing entry is
        // updated in place, and any same-named entries in scopes nested deeper
        // than the declaring one are dropped as shadowing duplicates.
        let mut found: Option<usize> = None;
        for (index, frame) in self.scope_stack.iter().enumerate() {
            if frame.owned_locals.iter().any(|(n, _, _)| n == name) {
                found = Some(index);
                break;
            }
        }
        match found {
            Some(index) => {
                self.scope_stack[index]
                    .owned_locals
                    .retain(|(n, _, _)| n != name);
                self.scope_stack[index]
                    .owned_locals
                    .push((name.to_string(), slot, ty));
            }
            None => self
                .frame_mut()
                .owned_locals
                .push((name.to_string(), slot, ty)),
        }
    }

    /// Record that a local's value escaped and must not be released here.
    pub(crate) fn mark_local_escaped(&mut self, name: &str) {
        self.frame_mut().escaped_locals.insert(name.to_string());
    }

    /// Stop the innermost scope's exit walk: the rest of the body is not on
    /// this path.
    pub(crate) fn block_scope_exit(&mut self) {
        self.frame_mut().blocked = true;
    }

    /// Release every tracked local of the innermost scope that is still ours.
    pub(crate) fn release_scope_locals(&mut self) {
        let (pending, escaped) = {
            let Some(frame) = self.scope_stack.last_mut() else {
                return;
            };
            if frame.blocked {
                return;
            }
            (
                std::mem::take(&mut frame.owned_locals),
                std::mem::take(&mut frame.escaped_locals),
            )
        };
        if pending.is_empty() {
            return;
        }
        for (name, slot, ty) in pending {
            if escaped.contains(&name) {
                continue;
            }
            // A local whose slot was reused by another name is not ours.
            match self.vars.get(&name) {
                Some(current) if *current == slot => {}
                _ => continue,
            }
            let ownership = super::stmt::ownership_of(&ty);
            if matches!(
                ownership,
                crate::compiler::mir::Ownership::Borrowed | crate::compiler::mir::Ownership::None
            ) {
                continue;
            }
            let temp = self.new_temp();
            let block = self.current_block;
            self.func.blocks[block].push(
                Some(temp),
                MirOp::Load(
                    MirValue::temp(slot),
                    MirType {
                        data_type: ty.clone(),
                    },
                ),
                (0, 0),
            );
            self.func.blocks[block].push(
                None,
                MirOp::Drop(MirValue::temp(temp), ownership),
                (0, 0),
            );
        }
    }

    /// Emit releases for the values owned on this return path without removing
    /// their ownership records from the lowering state. This is what a `return`
    /// needs while lowering a control-flow branch.
    ///
    /// `return` used to call `end_scope_and_release`, which releases one frame
    /// and pops it. At the top level of a function body that frame IS the
    /// function's own frame, so every top-level `return` tore the function
    /// scope down. Statements after it then ran against a stack that
    /// `frame_mut` silently refilled, and a local declared before the first
    /// `return` and rebound after it — `set out = []` followed by
    /// `set out = vec::push::str(out, x)` — was tracked in a second frame and
    /// released a second time. That is a double free, and it is why
    /// `owl info --json` aborted with one.
    ///
    /// Removing records here made an early return in one branch erase ownership
    /// from the sibling/fallthrough path. A later rebind then registered that
    /// outer local in an inner loop scope and freed it on every iteration. Keep
    /// the records for that fallthrough path; the drops remain in this return
    /// block, so the returning path still releases each owned local once.
    pub(crate) fn release_all_frames(&mut self) {
        let mut pending: Vec<(String, usize, DataType)> = Vec::new();
        let mut escaped: Vec<String> = Vec::new();
        for frame in self.scope_stack.iter_mut() {
            if frame.blocked {
                continue;
            }
            pending.extend(frame.owned_locals.iter().cloned());
            escaped.extend(std::mem::take(&mut frame.escaped_locals));
        }
        if pending.is_empty() {
            return;
        }
        for (name, slot, ty) in pending {
            if escaped.contains(&name) {
                continue;
            }
            // A local whose slot was reused by another name is not ours.
            match self.vars.get(&name) {
                Some(current) if *current == slot => {}
                _ => continue,
            }
            let ownership = super::stmt::ownership_of(&ty);
            if matches!(
                ownership,
                crate::compiler::mir::Ownership::Borrowed | crate::compiler::mir::Ownership::None
            ) {
                continue;
            }
            let temp = self.new_temp();
            let block = self.current_block;
            self.func.blocks[block].push(
                Some(temp),
                MirOp::Load(
                    MirValue::temp(slot),
                    MirType {
                        data_type: ty.clone(),
                    },
                ),
                (0, 0),
            );
            self.func.blocks[block].push(
                None,
                MirOp::Drop(MirValue::temp(temp), ownership),
                (0, 0),
            );
        }
    }
}

impl MirLower {
    /// Mark every local referenced by `expr` as having escaped.
    ///
    /// Used for a return expression: if the returned value reads a local, that
    /// local's storage (or a reference into it) is now reachable by the caller,
    /// so releasing it at scope exit would be a use-after-free.
    ///
    /// This walks only the expression forms that can name a local. A call whose
    /// result aliases a captured local is conservatively treated as escaping
    /// that local: over-marking costs a leak, while under-marking costs a
    /// use-after-free.
    pub(crate) fn mark_escaped_in_expression(&mut self, expr: &crate::parser::ast::Expression) {
        use crate::parser::ast::Expression as E;

        match expr {
            E::Identifier(id) => {
                let tracked = self
                    .scope_stack
                    .last()
                    .map(|frame| frame.owned_locals.iter().any(|(n, _, _)| *n == id.name))
                    .unwrap_or(false);
                if tracked {
                    self.mark_local_escaped(&id.name);
                }
            }
            E::MemberAccess { .. } => {
                // A field read may hand out a pointer into an owned struct.
                // Only tracked locals are marked, so an unrelated field read is
                // unaffected.
                let names: Vec<String> = self
                    .scope_stack
                    .last()
                    .map(|frame| {
                        frame
                            .owned_locals
                            .iter()
                            .map(|(n, _, _)| n.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                for name in names {
                    self.mark_local_escaped(&name);
                }
            }
            E::Call { args, .. } => {
                for arg in args {
                    self.mark_escaped_in_expression(arg);
                }
            }
            E::Ascription { expr, .. } => self.mark_escaped_in_expression(expr),
            E::BinaryOp { left, right, .. } => {
                self.mark_escaped_in_expression(left);
                self.mark_escaped_in_expression(right);
            }
            E::UnaryOp { operand, .. } => self.mark_escaped_in_expression(operand),
            E::Reference { expr, .. } => self.mark_escaped_in_expression(expr),
            E::Dereference { expr, .. } => self.mark_escaped_in_expression(expr),
            E::Index { .. } => {
                // Indexed reads can yield an interior pointer.
                let names: Vec<String> = self
                    .scope_stack
                    .last()
                    .map(|frame| {
                        frame
                            .owned_locals
                            .iter()
                            .map(|(n, _, _)| n.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                for name in names {
                    self.mark_local_escaped(&name);
                }
            }
            _ => {}
        }
    }
}
