use crate::error::diagnostic::{
    Diagnostic, DiagnosticCode, Label, LabelStyle, Severity, WarningFilter,
};
use crate::parser::Program;
use crate::parser::ast::{DataType, Expression, Identifier, Literal, Statement};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct WarningAnalyzer {
    diagnostics: Vec<Diagnostic>,
    filter: WarningFilter,
    deny: HashSet<DiagnosticCode>,
    defined_variables: HashSet<String>,
    variable_positions: HashMap<String, crate::error::Span>,
    used_variables: HashSet<String>,
    defined_functions: HashSet<String>,
    function_positions: HashMap<String, crate::error::Span>,
    used_functions: HashSet<String>,
    imported_modules: Vec<Identifier>,
    loop_depth: usize,
    current_span: crate::error::Span,
    statement_origins: Vec<PathBuf>,
    entry_path: Option<PathBuf>,
    suppress_library_warnings: bool,
    test_function_names: HashSet<String>,
    deprecated_functions: HashMap<String, String>,
    allow_dead_code: HashSet<String>,
    if_depth: usize,
    mutable_vars: HashSet<String>,
    mutated_vars: HashSet<String>,
}

impl WarningAnalyzer {
    pub fn new(filter: WarningFilter, deny: HashSet<DiagnosticCode>) -> Self {
        Self {
            diagnostics: Vec::new(),
            filter,
            deny,
            defined_variables: HashSet::new(),
            variable_positions: HashMap::new(),
            used_variables: HashSet::new(),
            defined_functions: HashSet::new(),
            function_positions: HashMap::new(),
            used_functions: HashSet::new(),
            imported_modules: Vec::new(),
            loop_depth: 0,
            if_depth: 0,
            mutable_vars: HashSet::new(),
            mutated_vars: HashSet::new(),
            current_span: crate::error::Span::new(1, 1),
            statement_origins: Vec::new(),
            entry_path: None,
            suppress_library_warnings: false,
            test_function_names: HashSet::new(),
            deprecated_functions: HashMap::new(),
            allow_dead_code: HashSet::new(),
        }
    }

    pub fn with_origins(mut self, statement_origins: &[PathBuf], entry_path: &Path) -> Self {
        self.statement_origins = statement_origins.to_vec();
        self.entry_path = Some(entry_path.to_path_buf());
        self.suppress_library_warnings = true;
        self
    }

    pub fn analyze(
        mut self,
        program: &Program,
        source: &str,
        filename: Option<&str>,
    ) -> Vec<Diagnostic> {
        for stmt in &program.statements {
            if let Statement::Function {
                name, attributes, ..
            } = stmt
            {
                if attributes.iter().any(|a| a.name == "test") {
                    self.test_function_names.insert(name.clone());
                }
                if let Some(dep) = attributes.iter().find(|a| a.name == "deprecated") {
                    let msg = dep
                        .args
                        .first()
                        .map(|a| a.value.clone())
                        .unwrap_or_else(|| "deprecated".to_string());
                    self.deprecated_functions.insert(name.clone(), msg);
                }
                if attributes.iter().any(|a| a.name == "allow")
                    && attributes.iter().any(|a| {
                        a.name == "allow" && a.args.iter().any(|arg| arg.value == "dead_code")
                    })
                {
                    self.allow_dead_code.insert(name.clone());
                }
            }
        }
        for (index, stmt) in program.statements.iter().enumerate() {
            if self.suppress_library_warnings {
                let origin = self.statement_origins.get(index);
                if let Some(entry) = &self.entry_path
                    && let Some(origin) = origin
                    && origin != entry
                {
                    continue;
                }
            }
            self.scan_defs(stmt);
        }
        for (index, stmt) in program.statements.iter().enumerate() {
            if self.suppress_library_warnings {
                let origin = self.statement_origins.get(index);
                if let Some(entry) = &self.entry_path
                    && let Some(origin) = origin
                    && origin != entry
                {
                    continue;
                }
            }
            self.scan_usage(stmt);
        }

        let defined_variables: Vec<String> = self.defined_variables.iter().cloned().collect();
        for name in &defined_variables {
            if !name.starts_with('_') && !self.used_variables.contains(name) {
                let pos = self
                    .variable_positions
                    .get(name)
                    .copied()
                    .filter(|s| !s.is_unknown())
                    .or_else(|| find_position_for_var(source, name));
                let Some(loc) = pos else {
                    continue;
                };
                self.push_warn_at(
                    DiagnosticCode::W0001,
                    "Unused Variable",
                    format!("Variable '{}' is never used", name),
                    loc,
                    name.len(),
                    Some("prefix with '_' to suppress this warning".to_string()),
                );
            }
        }

        let defined_functions: Vec<String> = self.defined_functions.iter().cloned().collect();
        for name in &defined_functions {
            if name != "main"
                && !name.starts_with('_')
                && !self.used_functions.contains(name)
                && !self.test_function_names.contains(name)
                && !self.allow_dead_code.contains(name)
            {
                let pos = self
                    .function_positions
                    .get(name)
                    .copied()
                    .filter(|s| !s.is_unknown())
                    .or_else(|| find_position_for_fn(source, name));
                let Some(loc) = pos else {
                    continue;
                };
                self.push_warn(
                    DiagnosticCode::W0002,
                    "Unused Function",
                    format!("Function '{}' is never used", name),
                    loc,
                    Some("prefix with '_' to suppress this warning".to_string()),
                );
            }
        }

        let imported_modules = self.imported_modules.clone();
        for load in &imported_modules {
            let _loc = if load.line == 0 && load.column == 0 {
                find_position_for_load(source, &load.name).unwrap_or(Span::unknown())
            } else {
                Span::new(load.line, load.column)
            };
            // W0003 removed: unused import check was impossible to trigger
        }

        let mutable_vars = self.mutable_vars.clone();
        for var in &mutable_vars {
            if !self.mutated_vars.contains(var)
                && let Some(&loc) = self.variable_positions.get(var)
            {
                self.push_warn(
                    DiagnosticCode::W0044,
                    "Unnecessary Mutable",
                    format!("Variable '{}' is declared `mut` but never reassigned", var),
                    loc,
                    Some("remove the `mut` modifier".to_string()),
                );
            }
        }

        self.check_deny_unsafe(program, filename);

        for diag in &mut self.diagnostics {
            diag.source = Some(source.to_string());
            if let Some(filename) = filename {
                diag.filename = Some(filename.to_string());
            }
        }
        self.diagnostics
    }

    fn scan_defs(&mut self, stmt: &Statement) {
        let loc = statement_location(stmt);
        if !loc.is_unknown() {
            self.current_span = loc;
        }
        match stmt {
            Statement::Let {
                name,
                data_type,
                value,
                is_mutable,
                ..
            } => {
                self.defined_variables.insert(name.clone());
                if !loc.is_unknown() {
                    self.variable_positions.insert(name.clone(), loc);
                }
                if *is_mutable {
                    self.mutable_vars.insert(name.clone());
                }
                if value.is_none() && *data_type != DataType::Unknown {
                    self.push_warn(
                        DiagnosticCode::W0041,
                        "Uninitialized Variable",
                        format!("Variable '{}' is declared without a value", name),
                        self.current_span,
                        Some("initialize with a value to avoid undefined behavior".to_string()),
                    );
                }
                if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    self.push_warn(
                        DiagnosticCode::W0034,
                        "Non-Idiomatic Variable Name",
                        format!(
                            "Variable '{}' starts with uppercase; prefer snake_case",
                            name
                        ),
                        self.current_span,
                        Some("rename to snake_case, e.g. `my_variable`".to_string()),
                    );
                }
                if *data_type == DataType::Unknown {
                    self.push_warn(
                        DiagnosticCode::W0004,
                        "Implicit Type Annotation",
                        format!("Variable '{}' relies on implicit typing", name),
                        self.current_span,
                        Some("consider adding an explicit type annotation: `:type`".to_string()),
                    );
                }
            }
            Statement::Function {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                self.defined_functions.insert(name.clone());
                if !loc.is_unknown() {
                    self.function_positions.insert(name.clone(), loc);
                }
                if name.chars().any(|c| c.is_ascii_uppercase()) {
                    self.push_warn(
                        DiagnosticCode::W0035,
                        "Non-Idiomatic Function Name",
                        format!(
                            "Function '{}' contains uppercase characters; prefer snake_case",
                            name
                        ),
                        self.current_span,
                        Some("rename to snake_case, e.g. `my_function`".to_string()),
                    );
                }
                if *return_type == DataType::Unknown {
                    self.push_warn(
                        DiagnosticCode::W0005,
                        "Implicit Return Type",
                        format!("Function '{}' has implicit return type", name),
                        self.current_span,
                        Some("consider declaring an explicit return type: `:type`".to_string()),
                    );
                }
                if body.is_empty() {
                    self.push_warn(
                        DiagnosticCode::W0006,
                        "Empty Function Body",
                        format!("Function '{}' has an empty body", name),
                        self.current_span,
                        Some("add statements to the function body".to_string()),
                    );
                }
                if body.len() > 60 {
                    self.push_warn(
                        DiagnosticCode::W0011,
                        "Long Function",
                        format!(
                            "Function '{}' is very long ({} statements)",
                            name,
                            body.len()
                        ),
                        self.current_span,
                        Some("consider splitting this function into smaller ones".to_string()),
                    );
                }
                if params.len() > 5 {
                    self.push_warn(
                        DiagnosticCode::W0012,
                        "Many Parameters",
                        format!("Function '{}' has many parameters ({})", name, params.len()),
                        self.current_span,
                        Some("consider grouping related parameters into a struct".to_string()),
                    );
                }
                if params.len() > 12 {
                    self.push_warn(
                        DiagnosticCode::W0037,
                        "Excessive Parameter Count",
                        format!(
                            "Function '{}' has {} parameters; consider grouping inputs",
                            name,
                            params.len()
                        ),
                        self.current_span,
                        Some("use a struct to group related parameters".to_string()),
                    );
                }
                if *return_type != DataType::None && !contains_explicit_return(body) {
                    self.push_warn(
                        DiagnosticCode::W0040,
                        "Missing Explicit Return",
                        format!(
                            "Function '{}' declares a return type but has no explicit return",
                            name
                        ),
                        self.current_span,
                        Some("add an explicit `return` statement for clarity".to_string()),
                    );
                }
                for b in body {
                    self.scan_defs(b);
                }
            }
            Statement::Load { path, .. } => {
                self.imported_modules.push(Identifier {
                    name: path.join("::"),
                    data_type: DataType::Unknown,
                    line: 0,
                    column: 0,
                });
            }
            Statement::LoadLocal { rel_path, .. } => {
                self.imported_modules.push(Identifier {
                    name: rel_path.join("::"),
                    data_type: DataType::Unknown,
                    line: 0,
                    column: 0,
                });
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                for s in then_branch {
                    self.scan_defs(s);
                }
                if let Some(else_branch) = else_branch {
                    for s in else_branch {
                        self.scan_defs(s);
                    }
                }
            }
            Statement::While { body, .. }
            | Statement::For { body, .. }
            | Statement::Find { body, .. } => {
                for s in body {
                    self.scan_defs(s);
                }
            }
            Statement::Match { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.scan_defs(s);
                    }
                }
                for s in default {
                    self.scan_defs(s);
                }
            }
            _ => {}
        }
    }

    fn scan_usage(&mut self, stmt: &Statement) {
        let loc = statement_location(stmt);
        if !loc.is_unknown() {
            self.current_span = loc;
        }
        match stmt {
            Statement::Expression(expr) => self.scan_expr(expr),
            Statement::Let {
                value: Some(value), ..
            } => self.scan_expr(value),
            Statement::Let { .. } => {}
            Statement::Assignment { target, value, .. } => {
                if let crate::parser::ast::AssignmentTarget::Variable(name) = target {
                    self.mutated_vars.insert(name.clone());
                }
                self.scan_expr(value);
            }
            Statement::Return(Some(expr)) => self.scan_expr(expr),
            Statement::Return(None) => {}
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.if_depth += 1;
                self.scan_expr(condition);
                if then_branch.is_empty() && else_branch.as_ref().is_none_or(|v| v.is_empty()) {
                    self.push_warn(
                        DiagnosticCode::W0014,
                        "Empty If Branches",
                        "if statement has empty branches".to_string(),
                        self.current_span,
                        Some("add statements or remove the empty if".to_string()),
                    );
                }
                if self.if_depth > 3 {
                    self.push_warn(
                        DiagnosticCode::W0043,
                        "Deeply Nested If",
                        format!("if statement at nesting depth {}", self.if_depth),
                        self.current_span,
                        Some("consider extracting to a function or using match".to_string()),
                    );
                }
                if is_return_bool(then_branch, true)
                    && else_branch
                        .as_ref()
                        .is_some_and(|e| is_return_bool(e, false))
                {
                    self.push_warn(
                        DiagnosticCode::W0046,
                        "Simplifiable if-return-bool",
                        "if cond { return true } else { return false } can be simplified to 'return cond'".to_string(),
                        self.current_span,
                        Some("use 'return cond' directly".to_string()),
                    );
                }
                for s in then_branch {
                    self.scan_usage(s);
                }
                if let Some(else_branch) = else_branch {
                    for s in else_branch {
                        self.scan_usage(s);
                    }
                }
                self.if_depth -= 1;
            }
            Statement::While { condition, body } => {
                self.loop_depth += 1;
                self.scan_expr(condition);
                if let Expression::Literal(Literal::Bool(true)) = condition
                    && !has_break(body)
                {
                    self.push_warn(
                        DiagnosticCode::W0042,
                        "Genuine Infinite Loop",
                        "while true loop has no break statement — will never exit".to_string(),
                        self.current_span,
                        Some(
                            "add a break condition or use a terminating loop construct".to_string(),
                        ),
                    );
                }
                if let Expression::Literal(Literal::Bool(false)) = condition {
                    self.push_warn(
                        DiagnosticCode::W0017,
                        "Unreachable Loop",
                        "while false body is unreachable".to_string(),
                        self.current_span,
                        Some("remove this loop or fix the condition".to_string()),
                    );
                }
                if self.loop_depth > 4 {
                    self.push_warn(
                        DiagnosticCode::W0018,
                        "Deep Loop Nesting",
                        format!("loop nesting depth is {}", self.loop_depth),
                        self.current_span,
                        Some("consider extracting inner loops into a function".to_string()),
                    );
                }
                if body.is_empty() {
                    self.push_warn(
                        DiagnosticCode::W0013,
                        "Empty Loop Body",
                        "loop has an empty body".to_string(),
                        self.current_span,
                        Some("add statements to the loop body or remove it".to_string()),
                    );
                }
                for s in body {
                    self.scan_usage(s);
                }
                self.loop_depth -= 1;
            }
            Statement::For {
                variable,
                iterable,
                body,
                ..
            } => {
                self.loop_depth += 1;
                self.scan_expr(iterable);
                if self.defined_variables.contains(variable) {
                    self.push_warn(
                        DiagnosticCode::W0039,
                        "Variable Shadowing",
                        format!("Loop variable '{}' shadows an existing binding", variable),
                        self.current_span,
                        Some("rename the loop variable to avoid confusion".to_string()),
                    );
                }
                if body.is_empty() {
                    self.push_warn(
                        DiagnosticCode::W0013,
                        "Empty Loop Body",
                        "loop has an empty body".to_string(),
                        self.current_span,
                        None,
                    );
                }
                for s in body {
                    self.scan_usage(s);
                }
                self.loop_depth -= 1;
            }
            Statement::Move { value, .. } => {
                self.scan_expr(value);
            }
            Statement::Drop { value } => {
                self.scan_expr(value);
            }
            Statement::New { value, .. } | Statement::Own { value, .. } => {
                if let Some(value) = value {
                    self.scan_expr(value);
                }
            }
            Statement::Break | Statement::Continue if self.loop_depth == 0 => {
                self.push_warn(
                    DiagnosticCode::W0019,
                    "Control Flow",
                    "break or continue used outside a loop".to_string(),
                    self.current_span,
                    Some("these statements are only valid inside loops".to_string()),
                );
            }
            Statement::Break | Statement::Continue => {}
            Statement::Load { .. } | Statement::LoadLocal { .. } => {
                // No-op: W0003 removed
            }
            Statement::Function { body, .. } => {
                for s in body {
                    self.scan_usage(s);
                }
            }
            Statement::Match {
                value,
                cases,
                default,
            } => {
                self.scan_expr(value);
                self.warn_duplicate_literal_patterns(cases);
                for (pat, body) in cases {
                    self.scan_expr(pat);
                    for s in body {
                        self.scan_usage(s);
                    }
                }
                for s in default {
                    self.scan_usage(s);
                }
            }
            _ => {}
        }
    }

    fn scan_expr(&mut self, expr: &Expression) {
        let loc = expression_location(expr);
        if !loc.is_unknown() {
            self.current_span = loc;
        }
        match expr {
            Expression::Identifier(id) => {
                self.used_variables.insert(id.name.clone());
            }
            Expression::Call { name, args, .. } => {
                self.used_functions.insert(name.clone());
                if name.contains('.') {
                    if let Some(base) = name.split('.').next() {
                        if self.defined_variables.contains(base) {
                            self.used_variables.insert(base.to_string());
                        }
                    }
                }
                let bare_name = name.split("::").last().unwrap_or(name);
                let dot_name = name.split('.').next_back().unwrap_or(name);
                if let Some(msg) = self
                    .deprecated_functions
                    .get(bare_name)
                    .or_else(|| self.deprecated_functions.get(dot_name))
                {
                    self.push_warn(
                        DiagnosticCode::W0010,
                        "Deprecated Function",
                        format!("'{}' is deprecated: {}", name, msg),
                        self.current_span,
                        None,
                    );
                }
                if args.len() > 16 {
                    self.push_warn(
                        DiagnosticCode::W0037,
                        "Large Call Arity",
                        format!("Call to '{}' has {} arguments", name, args.len()),
                        self.current_span,
                        Some("consider grouping arguments into a struct".to_string()),
                    );
                }
                for arg in args {
                    self.scan_expr(arg);
                }
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                ..
            } => {
                self.scan_expr(left);
                self.scan_expr(right);
                if operator == "==" && (is_bool_literal(left) || is_bool_literal(right)) {
                    self.push_warn(
                        DiagnosticCode::W0045,
                        "Redundant Boolean Comparison",
                        "comparing a value to true/false is redundant".to_string(),
                        self.current_span,
                        Some("use the value directly or negate with !".to_string()),
                    );
                }
                if operator == "+"
                    && self.loop_depth > 0
                    && let Expression::Identifier(id) = left.as_ref()
                    && is_str_type(id)
                {
                    self.push_warn(
                        DiagnosticCode::W0047,
                        "String Concatenation in Loop",
                        format!(
                            "'{}' is built via += inside a loop — consider lists::join",
                            id.name
                        ),
                        self.current_span,
                        Some(
                            "collect parts in a vec[str] and use join() after the loop".to_string(),
                        ),
                    );
                }
                if matches!(operator.as_str(), "==" | "!=" | "<=" | ">=" | "<" | ">")
                    && expr_fingerprint(left) == expr_fingerprint(right)
                {
                    self.push_warn(
                        DiagnosticCode::W0036,
                        "Self Comparison",
                        format!("Expression compares a value to itself with '{}'", operator),
                        self.current_span,
                        Some("this comparison is always true or always false".to_string()),
                    );
                }
                if let Expression::Literal(Literal::Int(n)) = right.as_ref() {
                    match operator.as_str() {
                        "*" if *n == 0 => self.push_warn(
                            DiagnosticCode::W0007,
                            "Multiplication by Zero",
                            "multiplication by zero always yields zero".to_string(),
                            self.current_span,
                            Some("this expression always evaluates to 0".to_string()),
                        ),
                        "/" if *n == 0 => self.push_warn(
                            DiagnosticCode::W0008,
                            "Division by Zero",
                            "division by zero is undefined".to_string(),
                            self.current_span,
                            Some("replace with a non-zero divisor".to_string()),
                        ),
                        "%" if *n == 0 => self.push_warn(
                            DiagnosticCode::W0009,
                            "Modulo by Zero",
                            "modulo by zero is undefined".to_string(),
                            self.current_span,
                            Some("replace with a non-zero divisor".to_string()),
                        ),
                        _ => {}
                    }
                }
            }
            Expression::UnaryOp { operand, .. }
            | Expression::Reference { expr: operand, .. }
            | Expression::Dereference { expr: operand, .. }
            | Expression::Box { value: operand, .. } => self.scan_expr(operand),
            Expression::List { elements, .. } => {
                for e in elements {
                    self.scan_expr(e);
                }
                if elements.len() > 128 {
                    self.push_warn(
                        DiagnosticCode::W0025,
                        "Large List Literal",
                        "large list literal may impact compile-time memory".to_string(),
                        self.current_span,
                        Some("consider building the list at runtime instead".to_string()),
                    );
                }
            }
            Expression::Dict { entries, .. } => {
                for (k, v) in entries {
                    self.scan_expr(k);
                    self.scan_expr(v);
                }
                if entries.len() > 64 {
                    self.push_warn(
                        DiagnosticCode::W0025,
                        "Large Dict Literal",
                        "large dict literal may impact compile-time memory".to_string(),
                        self.current_span,
                        Some("consider building the dict at runtime instead".to_string()),
                    );
                }
            }
            Expression::Index { target, index, .. } => {
                self.scan_expr(target);
                self.scan_expr(index);
                if let Expression::Literal(Literal::Int(n)) = index.as_ref()
                    && *n < 0
                {
                    self.push_warn(
                        DiagnosticCode::W0021,
                        "Negative Index",
                        "negative index may produce unexpected results".to_string(),
                        self.current_span,
                        Some("ensure the index is non-negative".to_string()),
                    );
                }
            }
            Expression::Literal(lit) => {
                if let Literal::Str(s) = lit
                    && s.len() > 120
                {
                    self.push_warn(
                        DiagnosticCode::W0024,
                        "Long String Literal",
                        format!("string literal is {} characters long", s.len()),
                        self.current_span,
                        Some("consider storing the string in a file or constant".to_string()),
                    );
                }
            }
            Expression::Tuple { elements, .. } => {
                for e in elements {
                    self.scan_expr(e);
                }
            }
            Expression::MemberAccess { target, .. }
            | Expression::Pipeline { input: target, .. } => self.scan_expr(target),
            Expression::Match {
                value,
                cases,
                default,
                ..
            } => {
                self.scan_expr(value);
                for (p, e) in cases {
                    self.scan_expr(p);
                    self.scan_expr(e);
                }
                self.scan_expr(default);
            }
            Expression::EnumVariant { payloads, .. } => {
                for p in payloads {
                    self.scan_expr(p);
                }
            }
            _ => {}
        }
    }

    fn push_warn(
        &mut self,
        code: DiagnosticCode,
        title: &str,
        message: String,
        span: crate::error::Span,
        help: Option<String>,
    ) {
        self.push_warn_at(code, title, message, span, 3, help);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_warn_at(
        &mut self,
        code: DiagnosticCode,
        title: &str,
        message: String,
        span: crate::error::Span,
        length: usize,
        help: Option<String>,
    ) {
        if !self.filter.matches(code) {
            return;
        }
        let severity = if self.deny.contains(&code) {
            Severity::Error
        } else {
            Severity::Warning
        };
        let mut diag = Diagnostic::new(severity, code, title, message, span);
        diag.labels.push(Label {
            span,
            length,
            message: "".to_string(),
            style: LabelStyle::Primary,
        });
        diag.help = help;
        self.diagnostics.push(diag);
    }

    fn warn_duplicate_literal_patterns(&mut self, cases: &[(Expression, Vec<Statement>)]) {
        let mut seen = HashSet::new();
        for (pat, _) in cases {
            if let Some(key) = literal_pattern_key(pat)
                && !seen.insert(key.clone())
            {
                self.push_warn(
                    DiagnosticCode::W0038,
                    "Duplicate Match Pattern",
                    format!("Duplicate literal pattern '{}' in match", key),
                    self.current_span,
                    Some("remove the duplicate pattern or merge with the first one".to_string()),
                );
            }
        }
    }

    fn check_deny_unsafe(&mut self, program: &Program, filename: Option<&str>) {
        let file_denies_unsafe = program
            .file_attributes
            .iter()
            .any(|a| a.name == "deny" && a.args.iter().any(|arg| arg.value == "unsafe"));

        for stmt in &program.statements {
            if let Statement::Function {
                name,
                body,
                attributes,
                ..
            } = stmt
            {
                let function_denies = attributes
                    .iter()
                    .any(|a| a.name == "deny" && a.args.iter().any(|arg| arg.value == "unsafe"));

                if !file_denies_unsafe && !function_denies {
                    continue;
                }

                if let Some(loc) = find_unsafe_block_position(body) {
                    let mut diag = Diagnostic::new(
                        Severity::Error,
                        DiagnosticCode::E0016,
                        "unsafe not allowed",
                        format!(
                            "Function '{}' contains an unsafe block but @[deny(unsafe)] forbids it",
                            name
                        ),
                        loc,
                    );
                    diag.labels.push(Label {
                        span: loc,
                        length: 6,
                        message: "unsafe block here".to_string(),
                        style: LabelStyle::Primary,
                    });
                    diag.help = Some(
                        "remove the unsafe block or remove the @[deny(unsafe)] attribute"
                            .to_string(),
                    );
                    if let Some(filename) = filename {
                        diag.filename = Some(filename.to_string());
                    }
                    self.diagnostics.push(diag);
                }
            }
        }
    }
}

fn contains_explicit_return(statements: &[Statement]) -> bool {
    for stmt in statements {
        match stmt {
            Statement::Return(_) => return true,
            Statement::If {
                then_branch,
                else_branch,
                ..
            } if contains_explicit_return(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| contains_explicit_return(branch)) =>
            {
                return true;
            }
            Statement::While { body, .. }
            | Statement::For { body, .. }
            | Statement::Find { body, .. }
            | Statement::Function { body, .. }
            | Statement::Unsafe { body, .. }
                if contains_explicit_return(body) =>
            {
                return true;
            }
            Statement::Match { cases, default, .. }
                if cases.iter().any(|(_, body)| contains_explicit_return(body))
                    || contains_explicit_return(default) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn literal_pattern_key(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Literal(Literal::Int(v)) => Some(format!("int:{v}")),
        Expression::Literal(Literal::Float(v)) => Some(format!("float:{v}")),
        Expression::Literal(Literal::Bool(v)) => Some(format!("bool:{v}")),
        Expression::Literal(Literal::Str(v)) => Some(format!("str:{v}")),
        Expression::Literal(Literal::Char(v)) => Some(format!("char:{v}")),
        Expression::Literal(Literal::None) => Some("mu".to_string()),
        _ => None,
    }
}

fn expr_fingerprint(expr: &Expression) -> String {
    match expr {
        Expression::Identifier(id) => format!("id:{}", id.name),
        Expression::Literal(Literal::Int(v)) => format!("int:{v}"),
        Expression::Literal(Literal::Float(v)) => format!("float:{v}"),
        Expression::Literal(Literal::Bool(v)) => format!("bool:{v}"),
        Expression::Literal(Literal::Str(v)) => format!("str:{v}"),
        Expression::Literal(Literal::Char(v)) => format!("char:{v}"),
        Expression::Literal(Literal::None) => "mu".to_string(),
        Expression::MemberAccess { target, member, .. } => {
            format!("member:{}:{}", expr_fingerprint(target), member)
        }
        Expression::Index { target, index, .. } => {
            format!(
                "index:{}:{}",
                expr_fingerprint(target),
                expr_fingerprint(index)
            )
        }
        _ => format!("{expr:?}"),
    }
}

fn has_break(statements: &[Statement]) -> bool {
    for stmt in statements {
        if matches!(stmt, Statement::Break) {
            return true;
        }
        if let Statement::If {
            then_branch,
            else_branch,
            ..
        } = stmt
        {
            if has_break(then_branch) {
                return true;
            }
            if let Some(else_b) = else_branch
                && has_break(else_b)
            {
                return true;
            }
        }
        if let Statement::While { body, .. }
        | Statement::For { body, .. }
        | Statement::Unsafe { body, .. } = stmt
            && has_break(body)
        {
            return true;
        }
    }
    false
}

fn is_bool_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::Literal(Literal::Bool(_)))
}

fn is_str_type(_ident: &Identifier) -> bool {
    true // In scan_expr we can't easily check types; warn on any += in loop
}

fn find_unsafe_block_position(body: &[Statement]) -> Option<crate::error::Span> {
    for stmt in body {
        if let Statement::Unsafe { .. } = stmt {
            return Some(statement_location(stmt));
        }
        match stmt {
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                if let Some(pos) = find_unsafe_block_position(then_branch) {
                    return Some(pos);
                }
                if let Some(else_b) = else_branch
                    && let Some(pos) = find_unsafe_block_position(else_b)
                {
                    return Some(pos);
                }
            }
            Statement::While { body, .. }
            | Statement::For { body, .. }
            | Statement::Find { body, .. }
            | Statement::Function { body, .. }
            | Statement::Unsafe { body, .. } => {
                if let Some(pos) = find_unsafe_block_position(body) {
                    return Some(pos);
                }
            }
            Statement::Match { cases, default, .. } => {
                for (_, body) in cases {
                    if let Some(pos) = find_unsafe_block_position(body) {
                        return Some(pos);
                    }
                }
                if let Some(pos) = find_unsafe_block_position(default) {
                    return Some(pos);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_return_bool(statements: &[Statement], expected: bool) -> bool {
    if statements.len() == 1
        && let Statement::Return(Some(Expression::Literal(Literal::Bool(val)))) = &statements[0]
    {
        return *val == expected;
    }
    false
}

use super::location::{expression_location, statement_location};
use crate::error::Span;

fn find_position_for_load(source: &str, module: &str) -> Option<Span> {
    find_position_for_any_pattern(
        source,
        &[
            &format!("load {} ", module),
            &format!("load {}\n", module),
            &format!("load {}", module),
        ],
    )
}

fn find_position_for_var(source: &str, name: &str) -> Option<Span> {
    for (idx, line) in source.lines().enumerate() {
        let mut search_start = 0;
        while let Some(col) = line[search_start..].find(name) {
            let abs_col = search_start + col;
            let before = abs_col.checked_sub(1).and_then(|i| line.as_bytes().get(i));
            let after = line.as_bytes().get(abs_col + name.len());
            let is_boundary = before.is_none_or(|&c| !c.is_ascii_alphanumeric() && c != b'_')
                && after.is_none_or(|&c| !c.is_ascii_alphanumeric() && c != b'_');
            if is_boundary {
                return Some(Span::new(idx + 1, abs_col + 1));
            }
            search_start = abs_col + 1;
        }
    }
    None
}

fn find_position_for_fn(source: &str, name: &str) -> Option<Span> {
    find_position_for_any_pattern(
        source,
        &[
            &format!("fn {}:", name),
            &format!("fn {} ", name),
            &format!("pub fn {}:", name),
            &format!("pub fn {} ", name),
        ],
    )
}

fn find_position_for_pattern(source: &str, pattern: &str) -> Option<Span> {
    for (idx, line) in source.lines().enumerate() {
        if let Some(col) = line.find(pattern) {
            return Some(Span::new(idx + 1, col + 1));
        }
    }
    None
}

fn find_position_for_any_pattern(source: &str, patterns: &[&str]) -> Option<Span> {
    for p in patterns {
        if let Some(pos) = find_position_for_pattern(source, p) {
            return Some(pos);
        }
    }
    None
}

pub fn check_warnings(
    program: &Program,
    source: &str,
    filename: Option<&str>,
    filter: WarningFilter,
    deny: HashSet<DiagnosticCode>,
) -> Vec<Diagnostic> {
    WarningAnalyzer::new(filter, deny).analyze(program, source, filename)
}

pub fn check_warnings_with_origins(
    program: &Program,
    source: &str,
    filename: Option<&str>,
    filter: WarningFilter,
    deny: HashSet<DiagnosticCode>,
    statement_origins: &[PathBuf],
    entry_path: &Path,
) -> Vec<Diagnostic> {
    WarningAnalyzer::new(filter, deny)
        .with_origins(statement_origins, entry_path)
        .analyze(program, source, filename)
}
