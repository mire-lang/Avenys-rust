use crate::parser::ast::Statement;

/// Flatten nested function definitions into top-level functions with
/// `parent::child` naming.
///
/// Given:
/// ```text
/// fn unwrap: () {
///     fn i64: (ptr :ptr) :i64 { return ... }
///     fn str: (ptr :ptr) :ptr { return ... }
/// }
/// ```
///
/// Produces:
/// ```text
/// fn unwrap: () { }
/// fn unwrap::i64: (ptr :ptr) :i64 { return ... }
/// fn unwrap::str: (ptr :ptr) :ptr { return ... }
/// ```
///
/// Handles arbitrary nesting depth: `fn unwrap: () { fn i64: () { fn or: ... } }`
/// becomes `unwrap::i64::or`.
///
/// Parent functions that contain ONLY nested function definitions become empty
/// namespace anchors. Parents that mix nested fns with other statements keep
/// their non-fn body and the children are promoted.
pub fn flatten_nested_functions(statements: &mut Vec<Statement>) {
    let mut index = 0;
    while index < statements.len() {
        if let Statement::Function { name, body, .. } = &statements[index] {
            let parent_name = name.clone();
            let (nested, remaining) = extract_nested_functions(body);

            if !nested.is_empty() {
                // Update parent body to remove nested fn children
                if let Statement::Function {
                    body: ref mut parent_body,
                    ..
                } = statements[index]
                {
                    *parent_body = remaining;
                }

                // Build flattened children
                let mut flattened = Vec::new();
                for child in nested {
                    flatten_one_child(&parent_name, child, &mut flattened);
                }

                // Insert flattened children after the parent
                let insert_at = index + 1;
                for (i, flat) in flattened.into_iter().enumerate() {
                    statements.insert(insert_at + i, flat);
                }

                // Skip past the inserted children
                index += 1;
                continue;
            }
        }
        index += 1;
    }
}

/// Process a single child function: prefix its name, check for grandchildren,
/// and push flattened results to `out`.
fn flatten_one_child(parent_name: &str, child: Statement, out: &mut Vec<Statement>) {
    if let Statement::Function {
        name: child_name,
        body: child_body,
        attributes,
        type_params,
        type_param_bounds,
        params,
        return_type,
        visibility,
        is_method,
        name_line,
        name_column,
    } = child
    {
        let full_name = format!("{}::{}", parent_name, child_name);
        let (grandchildren, _) = extract_nested_functions(&child_body);

        if !grandchildren.is_empty() {
            // The child itself is a namespace — emit it as empty
            // and promote grandchildren with the full prefix
            out.push(Statement::Function {
                name: full_name.clone(),
                attributes: vec![],
                type_params: vec![],
                type_param_bounds: vec![],
                params: vec![],
                body: vec![],
                return_type: crate::parser::ast::DataType::None,
                visibility: crate::parser::ast::Visibility::Public,
                is_method: false,
                name_line: 0,
                name_column: 0,
            });

            for grandchild in grandchildren {
                flatten_one_child(&full_name, grandchild, out);
            }
        } else {
            // Leaf child — emit with full name
            out.push(Statement::Function {
                name: full_name,
                attributes,
                type_params,
                type_param_bounds,
                params,
                body: child_body,
                return_type,
                visibility,
                is_method,
                name_line,
                name_column,
            });
        }
    }
}

/// Extract nested `Statement::Function` children from a function body.
/// Returns `(nested_functions, remaining_statements)`.
fn extract_nested_functions(body: &[Statement]) -> (Vec<Statement>, Vec<Statement>) {
    let mut nested = Vec::new();
    let mut remaining = Vec::new();

    for stmt in body {
        if let Statement::Function { .. } = stmt {
            nested.push(stmt.clone());
        } else {
            remaining.push(stmt.clone());
        }
    }

    (nested, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn find_functions(program: &crate::parser::Program) -> Vec<&Statement> {
        program
            .statements
            .iter()
            .filter(|s| matches!(s, Statement::Function { .. }))
            .collect()
    }

    fn fn_name(stmt: &Statement) -> &str {
        if let Statement::Function { name, .. } = stmt {
            name
        } else {
            panic!("expected Function statement")
        }
    }

    fn fn_body_len(stmt: &Statement) -> usize {
        if let Statement::Function { body, .. } = stmt {
            body.len()
        } else {
            panic!("expected Function statement")
        }
    }

    #[test]
    fn flattens_single_level_nesting() {
        let source = "\
pub fn unwrap: () {
    pub fn i64: (ptr :ptr) :i64 { return 42 }
    pub fn str: (ptr :ptr) :ptr { return ptr }
}
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        assert_eq!(fns.len(), 3, "expected unwrap, unwrap::i64, unwrap::str");
        assert_eq!(fn_name(fns[0]), "unwrap");
        assert_eq!(fn_name(fns[1]), "unwrap::i64");
        assert_eq!(fn_name(fns[2]), "unwrap::str");
        // Parent should be empty
        assert_eq!(fn_body_len(fns[0]), 0);
    }

    #[test]
    fn flattens_deeply_nested_functions() {
        let source = "\
pub fn unwrap: () {
    pub fn i64: () {
        pub fn or: (ptr :ptr, default :i64) :i64 { return default }
    }
}
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        let names: Vec<_> = fns.iter().map(|f| fn_name(f)).collect();
        assert!(
            names.contains(&"unwrap"),
            "should have unwrap, got {:?}",
            names
        );
        assert!(
            names.contains(&"unwrap::i64"),
            "should have unwrap::i64, got {:?}",
            names
        );
        assert!(
            names.contains(&"unwrap::i64::or"),
            "should have unwrap::i64::or, got {:?}",
            names
        );
    }

    #[test]
    fn preserves_non_fn_statements_in_parent_body() {
        let source = "\
pub fn unwrap: () {
    set x = 42
    pub fn i64: (ptr :ptr) :i64 { return 42 }
}
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        assert_eq!(fns.len(), 2, "expected unwrap and unwrap::i64");
        assert_eq!(fn_name(fns[0]), "unwrap");
        assert_eq!(fn_name(fns[1]), "unwrap::i64");
        // Parent should retain the non-fn statement
        assert_eq!(fn_body_len(fns[0]), 1);
    }

    #[test]
    fn no_op_when_no_nested_functions() {
        let source = "\
pub fn standalone: () { return 42 }
pub fn other: () { return 10 }
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        assert_eq!(fns.len(), 2);
        assert_eq!(fn_name(fns[0]), "standalone");
        assert_eq!(fn_name(fns[1]), "other");
    }

    #[test]
    fn flat_style_still_works() {
        let source = "\
pub fn unwrap::i64: (ptr :ptr) :i64 { return 42 }
pub fn unwrap::str: (ptr :ptr) :ptr { return ptr }
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        assert_eq!(fns.len(), 2);
        assert_eq!(fn_name(fns[0]), "unwrap::i64");
        assert_eq!(fn_name(fns[1]), "unwrap::str");
    }

    #[test]
    fn preserves_visibility_and_attributes() {
        let source = "\
pub fn group: () {
    fn private_child: () { return 1 }
    pub fn public_child: () { return 2 }
}
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        let private = fns.iter().find(|f| fn_name(f) == "group::private_child");
        let public = fns.iter().find(|f| fn_name(f) == "group::public_child");

        assert!(private.is_some(), "should have group::private_child");
        assert!(public.is_some(), "should have group::public_child");

        if let Statement::Function { visibility, .. } = private.unwrap() {
            assert_eq!(*visibility, crate::parser::ast::Visibility::Private);
        }
        if let Statement::Function { visibility, .. } = public.unwrap() {
            assert_eq!(*visibility, crate::parser::ast::Visibility::Public);
        }
    }

    #[test]
    fn mixed_flat_and_nested_styles_compose() {
        let source = "\
pub fn unwrap::i64::or: (ptr :ptr, default :i64) :i64 { return default }
pub fn unwrap: () {
    pub fn str: (ptr :ptr) :ptr { return ptr }
}
";
        let mut program = parse(source).expect("parse should succeed");
        flatten_nested_functions(&mut program.statements);

        let fns: Vec<_> = find_functions(&program);
        let names: Vec<_> = fns.iter().map(|f| fn_name(f)).collect();
        assert!(names.contains(&"unwrap::i64::or"), "flat style preserved");
        assert!(names.contains(&"unwrap"), "namespace anchor");
        assert!(
            names.contains(&"unwrap::str"),
            "nested flattened to unwrap::str"
        );
    }
}
