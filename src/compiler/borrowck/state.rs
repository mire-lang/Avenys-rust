use super::*;

impl<'a> BorrowChecker<'a> {
    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        if self.scopes.len() <= 1 {
            return;
        }

        if let Some(scope) = self.scopes.pop() {
            for (_, binding) in scope {
                for reference in binding.ref_targets {
                    self.release_borrow(&reference.target, reference.is_mutable);
                }
            }
        }
    }

    pub(super) fn insert_binding(&mut self, name: String, state: BindingState) {
        let previous = self
            .scopes
            .last_mut()
            .and_then(|scope| scope.insert(name, state));
        if let Some(previous) = previous {
            for reference in previous.ref_targets {
                self.release_borrow(&reference.target, reference.is_mutable);
            }
        }
    }

    pub(super) fn insert_match_pattern_bindings(&mut self, pattern: &Expression) {
        match pattern {
            Expression::EnumVariant { payloads, .. } => {
                for payload in payloads {
                    if let Expression::Identifier(ident) = payload {
                        self.insert_binding(ident.name.clone(), BindingState::default());
                    }
                }
            }
            Expression::Call { name, args, .. } if name == "__match_guard" => {
                if let Some(inner) = args.first() {
                    self.insert_match_pattern_bindings(inner);
                }
            }
            Expression::Call { name, args, .. } if name == "__match_or" => {
                if let Some(inner) = args.first() {
                    self.insert_match_pattern_bindings(inner);
                }
            }
            _ => {}
        }
    }

    pub(super) fn rebind_reference_targets(
        &mut self,
        name: &str,
        new_references: Vec<ReferenceBinding>,
    ) -> Result<()> {
        let old_references = self
            .lookup_binding(name)
            .map(|state| state.ref_targets.clone())
            .unwrap_or_default();
        for old in old_references {
            self.release_borrow(&old.target, old.is_mutable);
        }

        for new_ref in &new_references {
            self.register_borrow(&new_ref.target, new_ref.is_mutable)?;
        }

        if let Some(state) = self.lookup_binding_mut(name) {
            state.ref_targets = new_references;
        }

        Ok(())
    }

    pub(super) fn collect_ref_targets(&self, expression: &Expression) -> Vec<ReferenceBinding> {
        let mut targets = Vec::new();
        Self::collect_ref_targets_rec(expression, &mut targets);
        targets
    }

    fn collect_ref_targets_rec(expression: &Expression, targets: &mut Vec<ReferenceBinding>) {
        match expression {
            Expression::Reference {
                expr, is_mutable, ..
            } => {
                if let Some(name) = Self::identifier_name(expr) {
                    targets.push(ReferenceBinding {
                        target: name,
                        is_mutable: *is_mutable,
                    });
                }
            }
            Expression::Closure { capture, .. } => {
                for (name, _) in capture {
                    targets.push(ReferenceBinding {
                        target: name.clone(),
                        is_mutable: false,
                    });
                }
            }
            _ => {}
        }
    }

    pub(super) fn promote_temporary_borrows(&mut self, targets: &[ReferenceBinding]) {
        for target in targets {
            if let Some(pos) = self
                .temporary_borrows
                .iter()
                .position(|r| r.target == target.target && r.is_mutable == target.is_mutable)
            {
                self.temporary_borrows.remove(pos);
            }
        }
    }

    pub(super) fn merge_scopes(
        _before: &[HashMap<String, BindingState>],
        branch_a: &[HashMap<String, BindingState>],
        branch_b: &[HashMap<String, BindingState>],
    ) -> Vec<HashMap<String, BindingState>> {
        let mut merged = branch_a.to_vec();
        for i in 0..merged.len() {
            for (name, state) in merged[i].iter_mut() {
                if let Some(state_b) = branch_b[i].get(name) {
                    state.is_moved = state.is_moved || state_b.is_moved;
                }
            }
        }
        merged
    }

    pub(super) fn merge_multiple_scopes(
        before: &[HashMap<String, BindingState>],
        branches: &[Vec<HashMap<String, BindingState>>],
    ) -> Vec<HashMap<String, BindingState>> {
        if branches.is_empty() {
            return before.to_vec();
        }
        let mut merged = branches[0].clone();
        for i in 0..merged.len() {
            for (name, state) in merged[i].iter_mut() {
                for branch in &branches[1..] {
                    if let Some(state_b) = branch[i].get(name) {
                        state.is_moved = state.is_moved || state_b.is_moved;
                    }
                }
            }
        }
        merged
    }

    pub(super) fn lookup_binding(&self, name: &str) -> Option<&BindingState> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding);
            }
        }
        None
    }

    pub(super) fn lookup_binding_mut(&mut self, name: &str) -> Option<&mut BindingState> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                return Some(binding);
            }
        }
        None
    }

    pub(super) fn current_scope_depth(&self) -> usize {
        self.scopes.len().saturating_sub(1)
    }

    pub(super) fn current_function_scope_id(&self, name: &str) -> Option<usize> {
        self.semantic_model
            .functions
            .get(name)
            .or_else(|| {
                self.impl_owner_stack.last().and_then(|owner| {
                    self.semantic_model
                        .functions
                        .get(&format!("{owner}.{name}"))
                })
            })
            .map(|info| info.scope_id)
    }
}
