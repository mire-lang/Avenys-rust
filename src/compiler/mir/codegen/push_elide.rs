//! Elide the capacity check on `vec::push` calls that provably fit.
//!
//! `vec::with::capacity::i64(N)` allocates room for N elements. Every push is
//! lowered as `if len >= cap { grow } else { store; len += 1 }`, where `cap` is
//! read from the heap header. LLVM cannot remove that branch: the capacity
//! lives in memory that a growth call may rewrite, so even a loop that fills a
//! fresh vector with exactly N elements reloads and re-tests the capacity on
//! every push. On `alloc_pressure` (8 million iterations, 8 pushes each) the
//! branch is the whole difference between the Mire leg and a C loop that the
//! compiler can prove is in bounds: a hand-written C equivalent without the
//! check runs in 27 ms against 67 ms with it.
//!
//! This pass proves the check is dead for a given allocation site and returns
//! the push instructions that can then be lowered without it:
//!
//! 1. Find `vec::with::capacity::i64(<constant>)` calls (the allocation site).
//! 2. Follow the vector pointer through local allocas. Every use must be one
//!    of the vector operations below; anything that could leak the pointer
//!    (returning it, storing it in an aggregate, passing it to an unknown
//!    function) disqualifies the site.
//! 3. Bound how many times each push can execute. A push inside a loop is
//!    multiplied by the loop's trip count, which is recovered from the same
//!    induction variable pattern the MIR lowering emits for `while i < N`.
//!    Loops that also contain the allocation are skipped: each iteration gets
//!    a fresh vector, so they do not add pushes to one allocation's lifetime.
//! 4. If the total is at most the capacity, the pushes are safe to lower
//!    unchecked.
//!
//! The pass is conservative: any construct it cannot bound leaves the checked
//! lowering in place, so a failure to optimize never changes semantics.

use std::collections::{HashMap, HashSet};

use crate::compiler::mir::{
    MirBlock, MirConst, MirCmp, MirFunction, MirInst, MirOp, MirTerminator, MirValue,
};

/// Functions that allocate a vector with a known capacity.
const WITH_CAPACITY: &[&str] = &["vec.with.capacity.i64", "rt_vecs_with_capacity_i64"];

/// Push entry points. `vec.push.i64` is the stdlib wrapper the MIR still
/// carries at the call site; `rt_vecs_push_i64` is what the wrapper calls and
/// what the optimizer sometimes inlines into the caller.
const PUSHES: &[&str] = &["vec.push.i64", "rt_vecs_push_i64"];

/// Calls that may receive the vector pointer without it escaping the
/// function. Every one of them either reads the header or writes inside the
/// vector, and none of them retain the pointer or free the block.
const VECTOR_USERS: &[&str] = &[
    "vec.push.i64",
    "rt_vecs_push_i64",
    "vec.len",
    "rt_vecs_len",
    "vec.get.i64",
    "rt_vecs_get_i64",
    "vec.set.i64",
    "rt_vecs_set_i64",
    "vec.pop.i64",
    "rt_vecs_pop",
    "vec.first.i64",
    "rt_vecs_first",
    "vec.last.i64",
    "rt_vecs_last",
    "vec.sort",
    "rt_vecs_sort",
];

fn callee_name(value: &MirValue) -> Option<&str> {
    match value {
        MirValue::Global(name) => Some(name.as_str()),
        MirValue::FunctionRef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn is_push(name: &str) -> bool {
    PUSHES.contains(&name)
}

/// Every temp id mentioned by an operation.
fn temps_used(op: &MirOp) -> Vec<usize> {
    let mut out = Vec::new();
    let mut add = |v: &MirValue| {
        if let MirValue::Temp(t) = v {
            out.push(*t);
        }
    };
    match op {
        MirOp::Alloca(_) => {}
        MirOp::Load(v, _) => add(v),
        MirOp::Store(v, dst) => {
            add(v);
            add(dst);
        }
        MirOp::Add(a, b)
        | MirOp::Sub(a, b)
        | MirOp::Mul(a, b)
        | MirOp::SDiv(a, b)
        | MirOp::SRem(a, b)
        | MirOp::Shl(a, b)
        | MirOp::Shr(a, b)
        | MirOp::And(a, b)
        | MirOp::Or(a, b)
        | MirOp::Xor(a, b)
        | MirOp::BitAnd(a, b)
        | MirOp::BitOr(a, b)
        | MirOp::ICmp(_, a, b)
        | MirOp::FCmp(_, a, b) => {
            add(a);
            add(b);
        }
        MirOp::Call(callee, args, _) => {
            if let MirValue::FunctionRef { env, .. } = callee {
                add(env);
            } else {
                add(callee);
            }
            for arg in args {
                add(arg);
            }
        }
        MirOp::Gep(base, indices, _) => {
            add(base);
            for index in indices {
                add(index);
            }
        }
        MirOp::PtrToInt(v, _)
        | MirOp::IntToPtr(v, _)
        | MirOp::BitCast(v, _)
        | MirOp::ZExt(v, _)
        | MirOp::SExt(v, _)
        | MirOp::Trunc(v, _)
        | MirOp::Sitofp(v, _)
        | MirOp::Fptosi(v, _)
        | MirOp::Fptrunc(v, _)
        | MirOp::Fpext(v, _)
        | MirOp::Copy(v)
        | MirOp::Drop(v, _) => add(v),
        MirOp::Phi(values, _) => {
            for (v, _) in values {
                add(v);
            }
        }
        MirOp::Select(a, b, c) => {
            add(a);
            add(b);
            add(c);
        }
        MirOp::ExtractValue(v, index, _) => {
            add(v);
            add(index);
        }
        MirOp::InsertValue(v, value, _) => {
            add(v);
            add(value);
        }
        MirOp::Concat(values) => {
            for v in values {
                add(v);
            }
        }
    }
    out
}

/// Push result temps that are safe to lower without a capacity check.
pub(crate) fn elided_pushes(func: &MirFunction) -> HashSet<usize> {
    let mut allocas = HashSet::new();
    for block in &func.blocks {
        for inst in &block.insts {
            if let (Some(result), MirOp::Alloca(_)) = (&inst.result, &inst.op) {
                allocas.insert(*result);
            }
        }
    }

    // Every allocation site in the function. A site is only analysed when the
    // vector it produces is the sole value a local alloca ever holds: if two
    // `with_capacity` calls store into the same variable, the pushes after the
    // second one are charged to both sites, and a site with the larger capacity
    // would wrongly vouch for pushes that belong to the smaller vector.
    let mut sites = Vec::new();
    for block in &func.blocks {
        for inst in &block.insts {
            let (Some(root), MirOp::Call(callee, args, _)) = (&inst.result, &inst.op) else {
                continue;
            };
            let Some(name) = callee_name(callee) else {
                continue;
            };
            if !WITH_CAPACITY.contains(&name) || args.len() != 1 {
                continue;
            }
            let MirValue::Const(MirConst::Int(capacity)) = &args[0] else {
                continue;
            };
            if *capacity < 1 || *capacity > 1 << 20 {
                continue;
            }
            sites.push((*root, *capacity, block.id));
        }
    }
    let roots: HashSet<usize> = sites.iter().map(|(root, _, _)| *root).collect();

    // Which allocation sites store into which local alloca. A site is only
    // analysed when the alloca that carries its vector was never written by a
    // different site: otherwise the pushes after the next allocation are
    // charged to both, and the site with the larger capacity would wrongly
    // vouch for pushes that belong to the smaller vector.
    let mut writers: HashMap<usize, HashSet<usize>> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.insts {
            let MirOp::Store(MirValue::Temp(destination), MirValue::Temp(value)) = &inst.op else {
                continue;
            };
            if roots.contains(value) {
                writers.entry(*destination).or_default().insert(*value);
            }
        }
    }

    let mut out = HashSet::new();
    for (root, capacity, alloc_block) in sites {
        if let Some(pushes) = prove(func, root, capacity, &allocas, &writers, alloc_block) {
            out.extend(pushes);
        }
    }
    out
}

/// Returns the push instructions reachable from `root` when they provably fit
/// in `capacity`. `None` means "not proven", never "unsafe".
fn prove(
    func: &MirFunction,
    root: usize,
    capacity: i64,
    allocas: &HashSet<usize>,
    writers: &HashMap<usize, HashSet<usize>>,
    alloc_block: usize,
) -> Option<Vec<usize>> {
    let mut temps: HashSet<usize> = HashSet::from([root]);
    let mut carriers: HashSet<usize> = HashSet::new();

    // Follow the vector through local allocas until the sets stop growing.
    loop {
        let mut changed = false;
        for block in &func.blocks {
            for inst in &block.insts {
                match &inst.op {
                    MirOp::Load(MirValue::Temp(address), _) => {
                        if carriers.contains(address) {
                            if let Some(result) = inst.result {
                                changed |= temps.insert(result);
                            }
                        } else if temps.contains(address) {
                            return None;
                        }
                    }
                    MirOp::Store(destination, value) => {
                        if let MirValue::Temp(t) = value {
                            if temps.contains(t) {
                                match destination {
                                    MirValue::Temp(alloca) if allocas.contains(alloca) => {
                                        changed |= carriers.insert(*alloca);
                                    }
                                    _ => return None,
                                }
                            }
                        }
                        if let MirValue::Temp(address) = destination {
                            if temps.contains(address) {
                                return None;
                            }
                        }
                    }
                    MirOp::Drop(MirValue::Temp(value), _) => {
                        if !temps.contains(value) && carriers.contains(value) {
                            return None;
                        }
                    }
                    MirOp::Call(callee, args, _) => {
                        if let MirValue::FunctionRef { env, .. } = callee {
                            if let MirValue::Temp(env_temp) = **env {
                                if temps.contains(&env_temp) {
                                    return None;
                                }
                            }
                        }
                        let carries = args
                            .iter()
                            .any(|arg| matches!(arg, MirValue::Temp(t) if temps.contains(t)));
                        if carries {
                            let name = callee_name(callee)?;
                            if !VECTOR_USERS.contains(&name) {
                                return None;
                            }
                            if is_push(name) {
                                if let Some(result) = inst.result {
                                    changed |= temps.insert(result);
                                }
                            }
                        }
                    }
                    MirOp::Copy(MirValue::Temp(value)) => {
                        if temps.contains(value) {
                            if let Some(result) = inst.result {
                                changed |= temps.insert(result);
                            }
                        }
                    }
                    MirOp::Phi(values, _) => {
                        if values
                            .iter()
                            .any(|(v, _)| matches!(v, MirValue::Temp(t) if temps.contains(t)))
                        {
                            if let Some(result) = inst.result {
                                changed |= temps.insert(result);
                            }
                        }
                    }
                    _ => {
                        if temps_used(&inst.op).iter().any(|t| temps.contains(t)) {
                            return None;
                        }
                    }
                }

                // A carrier alloca may only be read and written directly. If
                // its address is taken, passed to a call, or dropped as a
                // value, the vector escapes this function.
                if temps_used(&inst.op).iter().any(|t| carriers.contains(t)) {
                    let direct = matches!(&inst.op, MirOp::Load(MirValue::Temp(a), _) if carriers.contains(a))
                        || matches!(&inst.op, MirOp::Store(MirValue::Temp(a), _) if carriers.contains(a));
                    if !direct {
                        return None;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Every alloca that carries this vector must be written by this site and
    // no other allocation site.
    for carrier in &carriers {
        if let Some(written_by) = writers.get(carrier) {
            if written_by.iter().any(|other| *other != root) {
                return None;
            }
        }
    }

    let cfg = ControlFlow::new(func);
    let loops = cfg.natural_loops();

    // A loop that contains the allocation starts from a fresh vector each
    // iteration, so it does not multiply the pushes of a single allocation.
    // That is only true when the allocation block is the loop's body entry;
    // otherwise the allocation may be conditional and an older vector could be
    // pushed again on later iterations.
    for lp in &loops {
        if lp.blocks.contains(&alloc_block) && !cfg.is_body_entry(lp, alloc_block) {
            return None;
        }
    }

    let mut total: i64 = 0;
    let mut pushes = Vec::new();
    let dominators = cfg.dominators();
    for block in &func.blocks {
        for inst in &block.insts {
            let MirOp::Call(callee, args, _) = &inst.op else {
                continue;
            };
            let Some(name) = callee_name(callee) else {
                continue;
            };
            if !is_push(name) {
                continue;
            }
            let Some(MirValue::Temp(vector)) = args.first() else {
                continue;
            };
            if !temps.contains(vector) {
                continue;
            }
            // Every push must run on a path that has just created the vector.
            // Without this, a push in a loop header (before the allocation in
            // the body) would be charged to a vector left over from the
            // previous iteration and undercounted.
            if !dominators
                .get(&block.id)
                .is_some_and(|set| set.contains(&alloc_block))
            {
                return None;
            }

            let mut executions: i64 = 1;
            for lp in &loops {
                if !lp.blocks.contains(&block.id) {
                    continue;
                }
                if lp.blocks.contains(&alloc_block) {
                    continue;
                }
                let bound = lp.bound?;
                // The header runs once more than the body.
                let extra = if block.id == lp.header { 1 } else { 0 };
                executions = executions.saturating_mul(bound.saturating_add(extra));
            }
            total = total.saturating_add(executions);
            if total > capacity {
                return None;
            }
            if let Some(result) = inst.result {
                pushes.push(result);
            }
        }
    }

    Some(pushes)
}

struct LoopInfo {
    header: usize,
    blocks: HashSet<usize>,
    /// Maximum number of body executions, when the shape is recognized.
    bound: Option<i64>,
}

struct ControlFlow<'a> {
    successors: HashMap<usize, Vec<usize>>,
    predecessors: HashMap<usize, Vec<usize>>,
    terminators: HashMap<usize, MirTerminator>,
    defs: HashMap<usize, MirInst>,
    blocks: Vec<&'a MirBlock>,
}

impl<'a> ControlFlow<'a> {
    fn new(func: &'a MirFunction) -> Self {
        let mut successors = HashMap::new();
        let mut predecessors: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut terminators = HashMap::new();
        let mut defs = HashMap::new();
        let mut blocks = Vec::new();
        for block in &func.blocks {
            let succ = match &block.terminator {
                MirTerminator::Br(target) => vec![*target],
                MirTerminator::BrCond(_, t, f) => vec![*t, *f],
                _ => Vec::new(),
            };
            for target in &succ {
                predecessors.entry(*target).or_default().push(block.id);
            }
            successors.insert(block.id, succ);
            terminators.insert(block.id, block.terminator.clone());
            for inst in &block.insts {
                if let Some(result) = inst.result {
                    defs.insert(result, inst.clone());
                }
            }
            blocks.push(block);
        }
        Self {
            successors,
            predecessors,
            terminators,
            defs,
            blocks,
        }
    }

    /// Back edges found by a depth-first search; `(tail, header)`.
    fn back_edges(&self) -> Vec<(usize, usize)> {
        let mut state: HashMap<usize, u8> = HashMap::new();
        let mut edges = Vec::new();
        let mut roots: Vec<usize> = self.successors.keys().copied().collect();
        roots.sort_unstable();
        for root in roots {
            if state.get(&root).copied().unwrap_or(0) == 0 {
                self.dfs(root, &mut state, &mut edges);
            }
        }
        edges
    }

    fn dfs(&self, node: usize, state: &mut HashMap<usize, u8>, edges: &mut Vec<(usize, usize)>) {
        state.insert(node, 1);
        let mut succ = self.successors.get(&node).cloned().unwrap_or_default();
        succ.sort_unstable();
        for target in succ {
            match state.get(&target).copied().unwrap_or(0) {
                0 => self.dfs(target, state, edges),
                1 => edges.push((node, target)),
                _ => {}
            }
        }
        state.insert(node, 2);
    }

    fn natural_loops(&self) -> Vec<LoopInfo> {
        let mut loops: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (tail, header) in self.back_edges() {
            let mut body = HashSet::from([header]);
            let mut stack = vec![tail];
            while let Some(node) = stack.pop() {
                if !body.insert(node) {
                    continue;
                }
                if let Some(preds) = self.predecessors.get(&node) {
                    for pred in preds {
                        if *pred != header {
                            stack.push(*pred);
                        }
                    }
                }
            }
            loops.entry(header).or_default().extend(body);
        }

        let mut out: Vec<LoopInfo> = loops
            .into_iter()
            .map(|(header, blocks)| {
                let bound = self.loop_bound(header, &blocks);
                LoopInfo {
                    header,
                    blocks,
                    bound,
                }
            })
            .collect();
        out.sort_unstable_by_key(|lp| lp.header);
        out
    }

    /// Recovers `while <induction> <op> <constant>` trip counts. Only the
    /// straight-line two-block loop the MIR lowering emits is recognized; any
    /// other shape reports no bound and the caller keeps the checked push.
    fn loop_bound(&self, header: usize, blocks: &HashSet<usize>) -> Option<i64> {
        let terminator = self.terminators.get(&header)?;
        let (condition, t, f) = match terminator {
            MirTerminator::BrCond(condition, t, f) => (condition, *t, *f),
            _ => return None,
        };
        let MirValue::Temp(condition_temp) = condition else {
            return None;
        };
        let compare = self.defs.get(condition_temp)?;
        let MirOp::ICmp(cmp, left, right) = &compare.op else {
            return None;
        };
        let (value, constant) = match (left, right) {
            (MirValue::Temp(value), MirValue::Const(MirConst::Int(constant))) => {
                (*value, *constant)
            }
            (MirValue::Const(MirConst::Int(constant)), MirValue::Temp(value)) => {
                (*value, *constant)
            }
            _ => return None,
        };
        // Only `<` and `<=` with a positive step are recovered.
        let span = match cmp {
            MirCmp::Lt => constant,
            MirCmp::Le => constant + 1,
            _ => return None,
        };

        let load = self.defs.get(&value)?;
        let MirOp::Load(MirValue::Temp(induction), _) = &load.op else {
            return None;
        };
        let induction = *induction;

        // Exactly one initialisation before the loop and one increment inside,
        // both on the same alloca.
        let mut initial = None;
        let mut step = None;
        let mut updates = 0;
        for block in &self.blocks {
            for inst in &block.insts {
                let MirOp::Store(MirValue::Temp(stored), stored_value) = &inst.op else {
                    continue;
                };
                if *stored != induction {
                    continue;
                }
                if blocks.contains(&block.id) {
                    let MirValue::Temp(source) = stored_value else {
                        return None;
                    };
                    let def = self.defs.get(source)?;
                    let MirOp::Add(a, b) = &def.op else {
                        return None;
                    };
                    let (step_value, delta) = match (a, b) {
                        (MirValue::Temp(v), MirValue::Const(MirConst::Int(d))) => (*v, *d),
                        (MirValue::Const(MirConst::Int(d)), MirValue::Temp(v)) => (*v, *d),
                        _ => return None,
                    };
                    let load = self.defs.get(&step_value)?;
                    let MirOp::Load(MirValue::Temp(loaded), _) = &load.op else {
                        return None;
                    };
                    if *loaded != induction || delta <= 0 {
                        return None;
                    }
                    step = Some(delta);
                    updates += 1;
                } else {
                    let MirValue::Const(MirConst::Int(start)) = stored_value else {
                        return None;
                    };
                    if initial.is_some() {
                        return None;
                    }
                    initial = Some(*start);
                }
            }
        }
        if updates != 1 {
            return None;
        }

        let start = initial?;
        let step = step?;
        let iterations = if span <= start {
            0
        } else {
            (span - start + step - 1) / step
        };

        // The bound only holds when the loop body is a single block that runs
        // on every iteration. The header could otherwise skip the increment
        // and the loop would run longer than the arithmetic suggests.
        let body: Vec<usize> = blocks.iter().copied().filter(|b| *b != header).collect();
        if body.len() != 1 {
            return None;
        }
        let body_block = body[0];
        let successor_in_loop =
            |target: usize| blocks.contains(&target) && target != header;
        if !(successor_in_loop(t) || successor_in_loop(f)) {
            return None;
        }
        if !matches!(self.terminators.get(&body_block), Some(MirTerminator::Br(target)) if *target == header)
        {
            return None;
        }
        Some(iterations)
    }

    /// Dominator sets for every block, computed by the usual iterative
    /// intersection over predecessors. The entry block is the lowest-numbered
    /// block with no predecessors.
    fn dominators(&self) -> HashMap<usize, HashSet<usize>> {
        let all: HashSet<usize> = self.successors.keys().copied().collect();
        let entry = self
            .successors
            .keys()
            .copied()
            .filter(|b| self.predecessors.get(b).map(|p| p.is_empty()).unwrap_or(true))
            .min()
            .or_else(|| self.successors.keys().copied().min());
        let Some(entry) = entry else {
            return HashMap::new();
        };
        let mut dom: HashMap<usize, HashSet<usize>> = all
            .iter()
            .map(|b| {
                if *b == entry {
                    (*b, HashSet::from([entry]))
                } else {
                    (*b, all.clone())
                }
            })
            .collect();
        loop {
            let mut changed = false;
            for block in all.iter().copied() {
                if block == entry {
                    continue;
                }
                let preds = self.predecessors.get(&block).cloned().unwrap_or_default();
                let mut next = if preds.is_empty() {
                    HashSet::from([block])
                } else {
                    let mut iter = preds.iter().filter_map(|p| dom.get(p));
                    let first = iter.next().cloned().unwrap_or_else(|| all.clone());
                    iter.fold(first, |acc, other| {
                        acc.intersection(other).copied().collect()
                    })
                };
                next.insert(block);
                if dom.get(&block) != Some(&next) {
                    dom.insert(block, next);
                    changed = true;
                }
            }
            if !changed {
                return dom;
            }
        }
    }

    /// True when `block` is the successor of the loop header that lies inside
    /// the loop, i.e. the first block executed on every iteration.
    fn is_body_entry(&self, lp: &LoopInfo, block: usize) -> bool {        let Some(terminator) = self.terminators.get(&lp.header) else {
            return false;
        };
        match terminator {
            MirTerminator::Br(target) => *target == block,
            MirTerminator::BrCond(_, t, f) => {
                let inside =
                    |target: usize| lp.blocks.contains(&target) && target != lp.header;
                (inside(*t) && *t == block) || (inside(*f) && *f == block)
            }
            _ => false,
        }
    }
}
