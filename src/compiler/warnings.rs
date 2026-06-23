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
    variable_positions: HashMap<String, (usize, usize)>,
    used_variables: HashSet<String>,
    defined_functions: HashSet<String>,
    function_positions: HashMap<String, (usize, usize)>,
    used_functions: HashSet<String>,
    imported_modules: Vec<Identifier>,
    used_imports: HashSet<String>,
    loop_depth: usize,
    current_line: usize,
    current_column: usize,
    statement_origins: Vec<PathBuf>,
    entry_path: Option<PathBuf>,
    suppress_library_warnings: bool,
}

impl WarningAnalyzer {
    pub fn new(
        filter: WarningFilter,
        deny: HashSet<DiagnosticCode>,
    ) -> Self {
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
            used_imports: HashSet::new(),
            loop_depth: 0,
            current_line: 1,
            current_column: 1,
            statement_origins: Vec::new(),
            entry_path: None,
            suppress_library_warnings: false,
        }
    }

    pub fn with_origins(
        mut self,
        statement_origins: &[PathBuf],
        entry_path: &Path,
    ) -> Self {
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
        for (index, stmt) in program.statements.iter().enumerate() {
            if self.suppress_library_warnings {
                let origin = self.statement_origins.get(index);
                if let Some(entry) = &self.entry_path
                    && let Some(origin) = origin
                        && origin != entry {
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
                        && origin != entry {
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
                    .filter(|(l, c)| !(*l == 1 && *c == 1))
                    .or_else(|| find_position_for_var(source, name));
                let Some((line, column)) = pos else {
                    continue;
                };
                self.push_warn_at(
                    DiagnosticCode::W0001,
                    "Unused Variable",
                    format!("Variable '{}' is never used", name),
                    line,
                    column,
                    name.len(),
                    Some("prefix with '_' to suppress this warning".to_string()),
                );
            }
        }

        let defined_functions: Vec<String> = self.defined_functions.iter().cloned().collect();
        for name in &defined_functions {
            if name != "main" && !name.starts_with('_') && !self.used_functions.contains(name) {
                let pos = self
                    .function_positions
                    .get(name)
                    .copied()
                    .filter(|(l, c)| !(*l == 1 && *c == 1))
                    .or_else(|| find_position_for_fn(source, name));
                let Some((line, column)) = pos else {
                    continue;
                };
                self.push_warn(
                    DiagnosticCode::W0002,
                    "Unused Function",
                    format!("Function '{}' is never used", name),
                    line,
                    column,
                    None,
                );
            }
        }

        let imported_modules = self.imported_modules.clone();
        for load in &imported_modules {
            if !self.used_imports.contains(&load.name) {
                let (line, column) = if load.line == 1 && load.column == 1 {
                    find_position_for_load(source, &load.name).unwrap_or((1, 1))
                } else {
                    (load.line, load.column)
                };
                self.push_warn(
                    DiagnosticCode::W0003,
                    "Unused Load",
                    format!("Load '{}' is never used", load.name),
                    line,
                    column,
                    None,
                );
            }
        }

        for diag in &mut self.diagnostics {
            diag.source = Some(source.to_string());
            if let Some(filename) = filename {
                diag.filename = Some(filename.to_string());
            }
        }
        self.diagnostics
    }

    fn scan_defs(&mut self, stmt: &Statement) {
        let (line, column) = statement_location(stmt);
        self.current_line = line;
        self.current_column = column;
        match stmt {
            Statement::Let {
                name, data_type, ..
            } => {
                self.defined_variables.insert(name.clone());
                self.variable_positions.insert(name.clone(), (line, column));
                if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    self.push_warn(
                        DiagnosticCode::W0034,
                        "Non-Idiomatic Variable Name",
                        format!(
                            "Variable '{}' starts with uppercase; prefer snake_case",
                            name
                        ),
                        1,
                        1,
                        None,
                    );
                }
                if *data_type == DataType::Unknown {
                    self.push_warn(
                        DiagnosticCode::W0004,
                        "Implicit Type Annotation",
                        format!("Variable '{}' relies on implicit typing", name),
                        1,
                        1,
                        None,
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
                self.function_positions.insert(name.clone(), (line, column));
                if name.chars().any(|c| c.is_ascii_uppercase()) {
                    self.push_warn(
                        DiagnosticCode::W0035,
                        "Non-Idiomatic Function Name",
                        format!(
                            "Function '{}' contains uppercase characters; prefer snake_case",
                            name
                        ),
                        1,
                        1,
                        None,
                    );
                }
                if *return_type == DataType::Unknown {
                    self.push_warn(
                        DiagnosticCode::W0005,
                        "Implicit Return Type",
                        format!("Function '{}' has implicit return type", name),
                        1,
                        1,
                        None,
                    );
                }
                if body.is_empty() {
                    self.push_warn(
                        DiagnosticCode::W0006,
                        "Empty Function Body",
                        format!("Function '{}' has an empty body", name),
                        1,
                        1,
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
                        1,
                        1,
                        None,
                    );
                }
                if params.len() > 5 {
                    self.push_warn(
                        DiagnosticCode::W0012,
                        "Many Parameters",
                        format!("Function '{}' has many parameters ({})", name, params.len()),
                        1,
                        1,
                        None,
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
                        1,
                        1,
                        None,
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
                        1,
                        1,
                        None,
                    );
                }
                for b in body {
                    self.scan_defs(b);
                }
            }
            Statement::Load {
                path,
                ..
            } => {
                self.imported_modules.push(Identifier {
                    name: path.join("::"),
                    data_type: DataType::Unknown,
                    line: 1,
                    column: 1,
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
        let (line, column) = statement_location(stmt);
        self.current_line = line;
        self.current_column = column;
        match stmt {
            Statement::Expression(expr) => self.scan_expr(expr),
            Statement::Assignment { value, .. } => {
                self.scan_expr(value);
                self.push_warn(
                    DiagnosticCode::W0029,
                    "Implicit Copy",
                    "implicit copy detected".to_string(),
                    1,
                    1,
                    None,
                );
            }
            Statement::Return(Some(expr)) => self.scan_expr(expr),
            Statement::Return(None) => {}
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.scan_expr(condition);
                if then_branch.is_empty() && else_branch.as_ref().is_none_or(|v| v.is_empty()) {
                    self.push_warn(
                        DiagnosticCode::W0014,
                        "Empty If Branches",
                        "if statement has empty branches".to_string(),
                        1,
                        1,
                        None,
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
            }
            Statement::While { condition, body } => {
                self.loop_depth += 1;
                self.scan_expr(condition);
                if let Expression::Literal(Literal::Bool(true)) = condition {
                    self.push_warn(
                        DiagnosticCode::W0016,
                        "Infinite Loop",
                        "while true can loop forever".to_string(),
                        1,
                        1,
                        None,
                    );
                }
                if let Expression::Literal(Literal::Bool(false)) = condition {
                    self.push_warn(
                        DiagnosticCode::W0017,
                        "Unreachable Loop",
                        "while false body is unreachable".to_string(),
                        1,
                        1,
                        None,
                    );
                }
                if self.loop_depth > 4 {
                    self.push_warn(
                        DiagnosticCode::W0018,
                        "Deep Loop Nesting",
                        format!("loop nesting depth is {}", self.loop_depth),
                        1,
                        1,
                        None,
                    );
                }
                if body.is_empty() {
                    self.push_warn(
                        DiagnosticCode::W0013,
                        "Empty Loop Body",
                        "loop has an empty body".to_string(),
                        1,
                        1,
                        None,
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
                        1,
                        1,
                        None,
                    );
                }
                if body.is_empty() {
                    self.push_warn(
                        DiagnosticCode::W0013,
                        "Empty Loop Body",
                        "loop has an empty body".to_string(),
                        1,
                        1,
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
                if !matches!(value, Expression::Call { name, .. } if name == "move::") {
                    self.push_warn(
                        DiagnosticCode::W0028,
                        "Implicit Move",
                        "implicit move; consider move::(x)".to_string(),
                        1,
                        1,
                        None,
                    );
                }
            }
            Statement::Drop { value } => {
                self.scan_expr(value);
                if !matches!(value, Expression::Call { name, .. } if name == "drop::") {
                    self.push_warn(
                        DiagnosticCode::W0030,
                        "Implicit Drop",
                        "implicit drop; consider drop::(x)".to_string(),
                        1,
                        1,
                        None,
                    );
                }
            }
            Statement::New { value, .. } | Statement::Own { value, .. } => {
                if let Some(value) = value {
                    self.scan_expr(value);
                } else {
                    self.push_warn(
                        DiagnosticCode::W0031,
                        "Unclear Ownership",
                        "explicit initialization can make ownership intent clearer".to_string(),
                        1,
                        1,
                        None,
                    );
                }
            }
            Statement::Break | Statement::Continue if self.loop_depth == 0 => {
                self.push_warn(
                    DiagnosticCode::W0019,
                    "Control Flow",
                    "break/continue outside loop".to_string(),
                    1,
                    1,
                    None,
                );
            }
            Statement::Break | Statement::Continue => {}
            Statement::Load { path, .. } => {
                self.used_imports.insert(path.join("::"));
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
        let (line, column) = expression_location(expr);
        self.current_line = line;
        self.current_column = column;
        match expr {
            Expression::Identifier(id) => {
                self.used_variables.insert(id.name.clone());
            }
            Expression::Call { name, args, .. } => {
                self.used_functions.insert(name.clone());
                if name == "clone" {
                    self.push_warn(
                        DiagnosticCode::W0027,
                        "Unnecessary Clone",
                        "unnecessary clone call".to_string(),
                        1,
                        1,
                        None,
                    );
                }
                if args.is_empty() && !self.defined_functions.contains(name) {
                    self.push_warn(
                        DiagnosticCode::W0020,
                        "Unknown Function Call",
                        format!("call to undefined function '{}'", name),
                        1,
                        1,
                        None,
                    );
                }
                if args.len() > 16 {
                    self.push_warn(
                        DiagnosticCode::W0037,
                        "Large Call Arity",
                        format!("Call to '{}' has {} arguments", name, args.len()),
                        1,
                        1,
                        None,
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
                if matches!(operator.as_str(), "==" | "!=" | "<=" | ">=" | "<" | ">")
                    && expr_fingerprint(left) == expr_fingerprint(right)
                {
                    self.push_warn(
                        DiagnosticCode::W0036,
                        "Self Comparison",
                        format!("Expression compares a value to itself with '{}'", operator),
                        1,
                        1,
                        None,
                    );
                }
                if let Expression::Literal(Literal::Int(n)) = right.as_ref() {
                    match operator.as_str() {
                        "*" if *n == 0 => self.push_warn(
                            DiagnosticCode::W0007,
                            "Multiplication by Zero",
                            "multiplication by zero".to_string(),
                            1,
                            1,
                            None,
                        ),
                        "/" if *n == 0 => self.push_warn(
                            DiagnosticCode::W0008,
                            "Division by Zero",
                            "division by zero".to_string(),
                            1,
                            1,
                            None,
                        ),
                        "%" if *n == 0 => self.push_warn(
                            DiagnosticCode::W0009,
                            "Modulo by Zero",
                            "modulo by zero".to_string(),
                            1,
                            1,
                            None,
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
                        "large list literal may impact memory".to_string(),
                        1,
                        1,
                        None,
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
                        "large dict literal may impact memory".to_string(),
                        1,
                        1,
                        None,
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
                        "negative index access".to_string(),
                        1,
                        1,
                        None,
                    );
                }
            }
            Expression::Literal(lit) => {
                if let Literal::Int(n) = lit {
                    if *n == 0 || *n == 1 {
                        self.push_warn(
                            DiagnosticCode::W0026,
                            "Magic Number",
                            "using literal 0/1 directly".to_string(),
                            1,
                            1,
                            None,
                        );
                    }
                    if *n < 0 {
                        self.push_warn(
                            DiagnosticCode::W0022,
                            "Negative Literal",
                            "negative literal used directly".to_string(),
                            1,
                            1,
                            None,
                        );
                    }
                }
                if let Literal::Str(s) = lit
                    && s.len() > 120
                {
                    self.push_warn(
                        DiagnosticCode::W0024,
                        "Long String Literal",
                        "very long string literal".to_string(),
                        1,
                        1,
                        None,
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
        line: usize,
        column: usize,
        help: Option<String>,
    ) {
        self.push_warn_at(code, title, message, line, column, 3, help);
    }

    fn push_warn_at(
        &mut self,
        code: DiagnosticCode,
        title: &str,
        message: String,
        line: usize,
        column: usize,
        length: usize,
        help: Option<String>,
    ) {
        if !self.filter.matches(code) {
            return;
        }
        let (line, column) = if line == 1 && column == 1 {
            (self.current_line.max(1), self.current_column.max(1))
        } else {
            (line.max(1), column.max(1))
        };
        let severity = if self.deny.contains(&code) {
            Severity::Error
        } else {
            Severity::Warning
        };
        let mut diag = Diagnostic::new(severity, code, title, message, line, column);
        diag.labels.push(Label {
            line,
            column,
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
                    1,
                    1,
                    None,
                );
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
            | Statement::Unsafe { body }
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

use super::location::{expression_location, statement_location};

fn find_position_for_load(source: &str, module: &str) -> Option<(usize, usize)> {
    find_position_for_any_pattern(
        source,
        &[
            &format!("load {} ", module),
            &format!("load {}\n", module),
            &format!("load {}", module),
        ],
    )
}

fn find_position_for_var(source: &str, name: &str) -> Option<(usize, usize)> {
    for (idx, line) in source.lines().enumerate() {
        let mut search_start = 0;
        while let Some(col) = line[search_start..].find(name) {
            let abs_col = search_start + col;
            let before = abs_col.checked_sub(1).and_then(|i| line.as_bytes().get(i));
            let after = line.as_bytes().get(abs_col + name.len());
            let is_boundary = before.is_none_or(|&c| !c.is_ascii_alphanumeric() && c != b'_')
                && after.is_none_or(|&c| !c.is_ascii_alphanumeric() && c != b'_');
            if is_boundary {
                return Some((idx + 1, abs_col + 1));
            }
            search_start = abs_col + 1;
        }
    }
    None
}

fn find_position_for_fn(source: &str, name: &str) -> Option<(usize, usize)> {
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

fn find_position_for_pattern(source: &str, pattern: &str) -> Option<(usize, usize)> {
    for (idx, line) in source.lines().enumerate() {
        if let Some(col) = line.find(pattern) {
            return Some((idx + 1, col + 1));
        }
    }
    None
}

fn find_position_for_any_pattern(source: &str, patterns: &[&str]) -> Option<(usize, usize)> {
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
