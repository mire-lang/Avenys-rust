#[cfg(test)]
use super::analysis::dependency_matches_unit;
use super::*;

mod expressions;
mod primitives;
mod statements;
mod types;
use expressions::*;
use primitives::*;
use statements::*;
use types::*;

#[cfg(test)]
pub(super) fn stable_statement_hash(statement: &Statement) -> u64 {
    let mut hasher = FxHasher::new();
    hash_statement(statement, &mut hasher);
    hasher.finish()
}

pub(super) fn stable_statement_hash_pair(statement: &Statement) -> (u64, u64) {
    let h1 = {
        let mut hasher = FxHasher::new();
        hash_statement(statement, &mut hasher);
        hasher.finish()
    };
    let h2 = {
        let mut hasher = FxHasher::with_seed(0x9e3779b97f4a7c15);
        hash_statement(statement, &mut hasher);
        hasher.finish()
    };
    (h1, h2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{DataType, Expression, Identifier, Literal, Visibility};
    use crate::parser::parse;
    use std::fs;

    fn demo_program(name: &str) -> Program {
        Program {
            annotations: Vec::new(),
            file_attributes: Vec::new(),
            statements: vec![Statement::Function {
                name: name.to_string(),
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
                params: Vec::new(),
                body: Vec::new(),
                return_type: crate::parser::ast::DataType::None,
                visibility: crate::parser::ast::Visibility::Public,
                is_method: false,
                attributes: Vec::new(),
                name_line: 0,
                name_column: 0,
            }],
        }
    }

    fn test_settings() -> CacheSettings {
        CacheSettings {
            max_units: Some(256),
            analysis_cache: true,
            compression: false,
            blob_checksum: false,
        }
    }

    fn make_cache_path(root: &Path) -> PathBuf {
        root.join("main.mire")
    }

    fn setup_test_root(root: &Path, source_path: &Path) {
        fs::create_dir_all(root).expect("temp dir");
        fs::write(source_path, "pub fn main: () {}\n").expect("source");
        fs::write(
            root.join("owl.toml"),
            "[project]\nname = \"test\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
        )
        .expect("owl.toml");
    }

    #[test]
    fn cache_roundtrips_parsed_and_analysis_entries() {
        let root = std::env::temp_dir().join(format!("mire_cache_test_{}", now_epoch_ms()));
        fs::create_dir_all(&root).expect("temp dir");
        let source_path = make_cache_path(&root);
        fs::write(&source_path, "pub fn main: () {}\n").expect("source");

        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");
        cache
            .store_file(
                &source_path,
                CachedParsedFile {
                    hash: 1,
                    hash2: 1,
                    program: demo_program("main"),
                    exports: vec!["main".to_string()],
                    local_imports: Vec::new(),
                },
            )
            .expect("store file");
        cache
            .store_analysis(&source_path, 0, 0, &demo_program("typed_main"))
            .expect("store analysis");
        cache.save().expect("save");

        let mut reloaded =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("reload");
        let parsed = reloaded
            .cached_file(&source_path, 1, 1)
            .expect("cached parsed file");
        assert_eq!(parsed.exports, vec!["main".to_string()]);
        let analyzed = reloaded
            .cached_analysis(&source_path, 0, 0)
            .expect("cached analysis");
        match analyzed {
            CachedAnalysis::Success(program) => assert_eq!(program.statements.len(), 1),
            CachedAnalysis::Error(err) => panic!("unexpected cached error: {err}"),
        }
    }

    #[test]
    fn cache_persists_across_reload() {
        let root = std::env::temp_dir().join(format!("mire_cache_persist_{}", now_epoch_ms()));
        fs::create_dir_all(&root).expect("temp dir");
        let source_path = make_cache_path(&root);
        fs::write(&source_path, "pub fn main: () {}\n").expect("source");

        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");
        cache
            .store_analysis(&source_path, 0, 0, &demo_program("typed_main"))
            .expect("store analysis");
        cache.save().expect("save");

        let mut reloaded =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("reload");
        assert!(reloaded.cached_analysis(&source_path, 0, 0).is_some());
    }

    #[test]
    fn cache_blob_checksum_rejects_tampered_blob_in_strict_mode() {
        let root = std::env::temp_dir().join(format!("mire_cache_checksum_{}", now_epoch_ms()));
        let source_path = make_cache_path(&root);
        setup_test_root(&root, &source_path);

        let settings = CacheSettings {
            max_units: Some(256),
            analysis_cache: true,
            compression: false,
            blob_checksum: true,
        };
        let mut cache = IncrementalCache::load_with_settings(&source_path, settings).expect("load");
        cache
            .store_file(
                &source_path,
                CachedParsedFile {
                    hash: 1,
                    hash2: 1,
                    program: demo_program("main"),
                    exports: vec!["main".to_string()],
                    local_imports: Vec::new(),
                },
            )
            .expect("store file");

        // Tamper with the stored blob. The blob filename IS its content
        // checksum, so any modification makes the read verify fail.
        let blobs_dir = cache.cache_dir().join("blobs");
        let blob_file = fs::read_dir(&blobs_dir)
            .expect("blobs dir")
            .filter_map(|e| e.ok())
            .next()
            .expect("a blob exists")
            .path();
        fs::write(&blob_file, b"tampered bytes").expect("tamper blob");

        // In checksum mode the corrupted blob is rejected as a cache miss.
        assert!(cache.cached_file(&source_path, 1, 1).is_none());

        // The corrupt blob is deleted so a re-store can rewrite it
        // (store_blob skips files that already exist).
        assert!(!blob_file.exists(), "corrupt blob should be removed");
    }

    #[test]
    fn cache_without_checksum_accepts_tampered_blob() {
        let root = std::env::temp_dir().join(format!("mire_cache_nocheck_{}", now_epoch_ms()));
        let source_path = make_cache_path(&root);
        setup_test_root(&root, &source_path);

        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");
        cache
            .store_file(
                &source_path,
                CachedParsedFile {
                    hash: 1,
                    hash2: 1,
                    program: demo_program("main"),
                    exports: vec!["main".to_string()],
                    local_imports: Vec::new(),
                },
            )
            .expect("store file");

        let blobs_dir = cache.cache_dir().join("blobs");
        let blob_file = fs::read_dir(&blobs_dir)
            .expect("blobs dir")
            .filter_map(|e| e.ok())
            .next()
            .expect("a blob exists")
            .path();
        fs::write(&blob_file, b"tampered bytes").expect("tamper blob");

        // Open mode trusts the cache: the tampered blob fails bincode
        // deserialization and is treated as a miss, but it is NOT deleted.
        assert!(cache.cached_file(&source_path, 1, 1).is_none());
        assert!(blob_file.exists(), "open mode keeps the blob");
    }

    #[test]
    fn lru_prunes_when_max_units_is_reached() {
        let root = std::env::temp_dir().join(format!("mire_cache_lru_{}", now_epoch_ms()));
        let source_path = make_cache_path(&root);
        setup_test_root(&root, &source_path);

        let settings = CacheSettings {
            max_units: Some(1),
            analysis_cache: true,
            compression: false,
            blob_checksum: false,
        };
        let mut cache = IncrementalCache::load_with_settings(&source_path, settings).expect("load");
        cache
            .store_file(
                &source_path,
                CachedParsedFile {
                    hash: 1,
                    hash2: 1,
                    program: demo_program("main"),
                    exports: vec!["main".to_string()],
                    local_imports: Vec::new(),
                },
            )
            .expect("store file");
        cache
            .store_analysis(&source_path, 0, 0, &demo_program("analysis"))
            .expect("store analysis");
        // With max_units=1, only one entry should survive
        assert!(
            cache.file_count() + cache.analysis_count() <= 2,
            "expected <= 2 entries, got files={} analyses={}",
            cache.file_count(),
            cache.analysis_count(),
        );
    }

    #[test]
    fn overwrite_analysis_does_not_grow_cache_indefinitely() {
        let root = std::env::temp_dir().join(format!("mire_cache_overwrite_{}", now_epoch_ms()));
        let source_path = make_cache_path(&root);
        setup_test_root(&root, &source_path);

        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");

        for i in 0..32 {
            let function_name = format!("main_{}", i);
            cache
                .store_analysis(&source_path, 0, 0, &demo_program(&function_name))
                .expect("store analysis overwrite");
        }

        // After 32 overwrites, only the latest analysis should be present
        assert_eq!(
            cache.analysis_count(),
            2,
            "overwrites should keep only 2 entries (main + latest)"
        );
    }

    #[test]
    fn cache_metrics_track_file_and_analysis_hits_and_misses() {
        let root = std::env::temp_dir().join(format!("mire_cache_metrics_{}", now_epoch_ms()));
        fs::create_dir_all(&root).expect("temp dir");
        let source_path = make_cache_path(&root);
        fs::write(&source_path, "pub fn main: () {}\n").expect("source");

        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");
        cache
            .store_file(
                &source_path,
                CachedParsedFile {
                    hash: 1,
                    hash2: 1,
                    program: demo_program("main"),
                    exports: vec!["main".to_string()],
                    local_imports: Vec::new(),
                },
            )
            .expect("store file");
        cache
            .store_analysis(&source_path, 0, 0, &demo_program("typed_main"))
            .expect("store analysis");

        assert!(cache.cached_file(&source_path, 1, 1).is_some());
        assert!(cache.cached_file(&source_path, 2, 2).is_none());
        assert!(cache.cached_analysis(&source_path, 0, 0).is_some());

        let metrics = cache.metrics();
        assert_eq!(metrics.file_hits, 1);
        assert_eq!(metrics.file_misses, 1);
        assert_eq!(metrics.analysis_hits, 1);
        assert_eq!(metrics.analysis_misses, 0);
    }

    #[test]
    fn cache_roundtrips_analysis_errors() {
        let root = std::env::temp_dir().join(format!("mire_cache_error_{}", now_epoch_ms()));
        fs::create_dir_all(&root).expect("temp dir");
        let source_path = make_cache_path(&root);
        fs::write(&source_path, "pub fn main: () {}\n").expect("source");

        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");
        let error = MireError::new(ErrorKind::Type {
            span: crate::error::Span::new(1, 1),
            message: "cached type failure".to_string(),
            code: None,
        })
        .with_filename(source_path.display().to_string())
        .with_source("pub fn main: () {}\n".to_string());
        cache
            .store_analysis_error(&source_path, 0, 0, &demo_program("broken"), &error)
            .expect("store error");
        cache.save().expect("save");

        let mut reloaded =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("reload");
        let cached = reloaded
            .cached_analysis(&source_path, 0, 0)
            .expect("cached analysis");
        match cached {
            CachedAnalysis::Success(_) => panic!("expected cached error"),
            CachedAnalysis::Error(err) => {
                assert!(matches!(err.kind, ErrorKind::Type { .. }));
                assert!(err.to_string().contains("cached type failure"));
            }
        }
    }

    #[test]
    fn load_with_settings_recovers_from_empty_cache() {
        let root = std::env::temp_dir().join(format!("mire_cache_empty_{}", now_epoch_ms()));
        let source_path = make_cache_path(&root);
        setup_test_root(&root, &source_path);

        // First load with no prior cache
        let mut cache =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("load");
        assert_eq!(cache.file_count(), 0);
        assert_eq!(cache.analysis_count(), 0);
        assert_eq!(cache.build_count(), 0);

        cache
            .store_analysis(&source_path, 0, 0, &demo_program("typed_main"))
            .expect("store analysis");
        cache.save().expect("save rebuilt cache");

        let mut reloaded =
            IncrementalCache::load_with_settings(&source_path, test_settings()).expect("reload");
        assert!(reloaded.cached_analysis(&source_path, 0, 0).is_some());
    }

    #[test]
    fn invalidation_report_marks_dependents_of_changed_function() {
        let previous = parse(
            "fn helper: () :i64 {\n    return 1\n}\nfn main: () :i64 {\n    return helper()\n}\n",
        )
        .expect("parse previous");
        let current = parse(
            "fn helper: () :i64 {\n    return 2\n}\nfn main: () :i64 {\n    return helper()\n}\n",
        )
        .expect("parse current");

        let report = compute_invalidation_report(
            &analysis_units_for_program(&previous),
            &analysis_units_for_program(&current),
        );

        assert_eq!(report.changed_units, vec!["helper".to_string()]);
        assert!(report.invalidated_units.contains(&"helper".to_string()));
        assert!(report.invalidated_units.contains(&"main".to_string()));
    }

    #[test]
    fn invalidation_report_marks_added_and_removed_units() {
        let previous = parse("fn helper: () :i64 {\n    return 1\n}\n").expect("parse previous");
        let current = parse(
            "fn helper: () :i64 {\n    return 1\n}\nfn main: () :i64 {\n    return helper()\n}\n",
        )
        .expect("parse current");

        let report = compute_invalidation_report(
            &analysis_units_for_program(&previous),
            &analysis_units_for_program(&current),
        );
        assert_eq!(report.added_units, vec!["main".to_string()]);
        assert!(report.invalidated_units.contains(&"main".to_string()));

        let reverse = compute_invalidation_report(
            &analysis_units_for_program(&current),
            &analysis_units_for_program(&previous),
        );
        assert_eq!(reverse.removed_units, vec!["main".to_string()]);
        assert!(reverse.invalidated_units.contains(&"main".to_string()));
    }

    #[test]
    fn invalidation_report_uses_latest_created_not_last_access() {
        let root =
            std::env::temp_dir().join(format!("mire_cache_latest_created_{}", now_epoch_ms()));
        let source_path = root.join("main.mire");
        setup_test_root(&root, &source_path);

        let settings = CacheSettings {
            max_units: Some(32),
            analysis_cache: true,
            compression: false,
            blob_checksum: false,
        };
        let mut cache = IncrementalCache::load_with_settings(&source_path, settings).expect("load");

        let older = parse(
            "fn helper: () :i64 {\n    return 1\n}\nfn main: () :i64 {\n    return helper()\n}\n",
        )
        .expect("parse older");
        cache
            .store_analysis(&source_path, 0, 0, &older)
            .expect("store older analysis");

        std::thread::sleep(std::time::Duration::from_millis(2));

        let newer = parse(
            "fn helper: () :i64 {\n    return 1\n}\nfn main: () :i64 {\n    return helper()\n}\nfn extra: () :i64 {\n    return 7\n}\n",
        )
        .expect("parse newer");
        cache
            .store_analysis(&source_path, 0, 0, &newer)
            .expect("store newer analysis");

        let report = cache
            .analysis_invalidation_report(&source_path, 0, &newer)
            .expect("report");
        assert!(
            report.changed_units.is_empty(),
            "must compare against newest created snapshot, got changed={:?}",
            report.changed_units
        );
        assert!(
            report.added_units.is_empty(),
            "must compare against newest created snapshot, got added={:?}",
            report.added_units
        );
    }

    #[test]
    fn analysis_units_include_nested_children_for_supported_containers() {
        let program = Program {
            file_attributes: vec![],
            annotations: vec![],
            statements: vec![
                Statement::Type {
                    visibility: Visibility::Public,
                    name: "PointType".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    parent: None,
                    fields: vec![Statement::Let {
                        name: "x".to_string(),
                        data_type: DataType::I64,
                        value: Some(Expression::Literal { lit: Literal::Int(1), line: 0, column: 0 }),
                        is_constant: false,
                        is_mutable: false,
                        is_static: false,
                        visibility: Visibility::Public,
                        name_line: 1,
                        name_column: 1,
                    }],
                },
                Statement::Impl {
                    trait_name: None,
                    type_name: "PointImpl".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    methods: vec![Statement::Function {
                        name: "new".to_string(),
                        type_params: Vec::new(),
                        type_param_bounds: Vec::new(),
                        params: vec![],
                        body: vec![],
                        return_type: DataType::None,
                        visibility: Visibility::Public,
                        is_method: true,
                        attributes: Vec::new(),
                        name_line: 0,
                        name_column: 0,
                    }],
                },
            ],
        };

        let units = analysis_units_for_program(&program);
        let keys: Vec<_> = units.into_iter().map(|unit| unit.unit_key).collect();

        assert!(keys.contains(&"PointType".to_string()));
        assert!(keys.contains(&"PointType#x".to_string()));
        assert!(keys.contains(&"impl::PointImpl".to_string()));
        assert!(keys.contains(&"PointImpl.new".to_string()));
    }

    #[test]
    fn stable_statement_hash_is_deterministic_for_same_statement() {
        let stmt = Statement::Function {
            name: "main".to_string(),
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            params: vec![("x".to_string(), DataType::I64)],
            body: vec![Statement::Return(Some(Expression::BinaryOp {
                left: Box::new(Expression::Identifier(Identifier {
                    name: "x".to_string(),
                    data_type: DataType::I64,
                    line: 0,
                    column: 0,
                })),
                operator: "+".to_string(),
                right: Box::new(Expression::Literal { lit: Literal::Int(1), line: 0, column: 0 }),
                data_type: DataType::I64,
            }))],
            return_type: DataType::I64,
            visibility: Visibility::Public,
            is_method: false,
            attributes: Vec::new(),
            name_line: 0,
            name_column: 0,
        };

        let h1 = stable_statement_hash(&stmt);
        let h2 = stable_statement_hash(&stmt);
        assert_eq!(h1, h2);
        assert_ne!(h1, 0);
    }

    #[test]
    fn stable_statement_hash_changes_when_statement_changes() {
        let stmt_a = Statement::Function {
            name: "main".to_string(),
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            params: Vec::new(),
            body: vec![Statement::Return(Some(Expression::Literal {
                lit: Literal::Int(1),
                line: 0,
                column: 0,
            }))],
            return_type: DataType::I64,
            visibility: Visibility::Public,
            is_method: false,
            attributes: Vec::new(),
            name_line: 0,
            name_column: 0,
        };
        let stmt_b = Statement::Function {
            name: "main".to_string(),
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            params: Vec::new(),
            body: vec![Statement::Return(Some(Expression::Literal {
                lit: Literal::Int(2),
                line: 0,
                column: 0,
            }))],
            return_type: DataType::I64,
            visibility: Visibility::Public,
            is_method: false,
            attributes: Vec::new(),
            name_line: 0,
            name_column: 0,
        };

        let h1 = stable_statement_hash(&stmt_a);
        let h2 = stable_statement_hash(&stmt_b);
        assert_ne!(h1, h2);
    }

    fn compute_invalidation_report_naive(
        previous_units: &[AnalysisUnitMetadata],
        current_units: &[AnalysisUnitMetadata],
    ) -> AnalysisInvalidationReport {
        let previous_by_key: HashMap<_, _> = previous_units
            .iter()
            .map(|unit| (unit.unit_key.clone(), unit))
            .collect();
        let current_by_key: HashMap<_, _> = current_units
            .iter()
            .map(|unit| (unit.unit_key.clone(), unit))
            .collect();

        let mut changed_units = Vec::new();
        let mut added_units = Vec::new();
        let mut removed_units = Vec::new();

        for (key, current) in &current_by_key {
            match previous_by_key.get(key) {
                Some(previous) => {
                    if previous.body_hash != current.body_hash
                        || previous.body_hash2 != current.body_hash2
                        || previous.dependencies != current.dependencies
                        || previous.unit_kind != current.unit_kind
                    {
                        changed_units.push(key.clone());
                    }
                }
                None => added_units.push(key.clone()),
            }
        }

        for key in previous_by_key.keys() {
            if !current_by_key.contains_key(key) {
                removed_units.push(key.clone());
            }
        }

        let mut invalidated: HashMap<String, ()> = HashMap::new();
        let mut queue = changed_units.clone();
        queue.extend(added_units.clone());
        queue.extend(removed_units.clone());

        while let Some(unit) = queue.pop() {
            if invalidated.insert(unit.clone(), ()).is_some() {
                continue;
            }

            for current in current_units {
                if current
                    .dependencies
                    .iter()
                    .any(|dep| dependency_matches_unit(dep, &unit))
                    && !invalidated.contains_key(&current.unit_key)
                {
                    queue.push(current.unit_key.clone());
                }
            }
        }

        let mut invalidated_units: Vec<_> = invalidated.into_keys().collect();
        changed_units.sort();
        added_units.sort();
        removed_units.sort();
        invalidated_units.sort();

        AnalysisInvalidationReport {
            changed_units,
            invalidated_units,
            added_units,
            removed_units,
        }
    }

    #[test]
    fn invalidation_report_indexed_matches_naive_behavior() {
        let mut previous = Vec::new();
        let mut current = Vec::new();
        let n = 300usize;

        for i in 0..n {
            let key = format!("Type{i}.run");
            let dep = if i == 0 {
                "seed".to_string()
            } else {
                format!("run{}", i - 1)
            };
            let unit_prev = AnalysisUnitMetadata {
                unit_key: key.clone(),
                unit_kind: AnalysisUnitKind::Function,
                body_hash: (1000 + i) as u64,
                body_hash2: (1000 + i) as u64,
                dependencies: vec![dep.clone()],
                origin: None,
            };
            let unit_curr = AnalysisUnitMetadata {
                body_hash: if i % 37 == 0 {
                    (2000 + i) as u64
                } else {
                    (1000 + i) as u64
                },
                ..unit_prev.clone()
            };
            previous.push(unit_prev);
            current.push(unit_curr);
        }

        current.push(AnalysisUnitMetadata {
            unit_key: "TypeExtra.run".to_string(),
            unit_kind: AnalysisUnitKind::Function,
            body_hash: 999_999,
            body_hash2: 999_999,
            dependencies: vec!["run299".to_string()],
            origin: None,
        });
        let _ = previous.pop();

        let indexed = compute_invalidation_report(&previous, &current);
        let naive = compute_invalidation_report_naive(&previous, &current);
        assert_eq!(indexed.changed_units, naive.changed_units);
        assert_eq!(indexed.added_units, naive.added_units);
        assert_eq!(indexed.removed_units, naive.removed_units);
        assert_eq!(indexed.invalidated_units, naive.invalidated_units);
    }

    #[test]
    fn invalidation_report_handles_large_dependency_chains() {
        let n = 4000usize;
        let mut previous = Vec::with_capacity(n);
        let mut current = Vec::with_capacity(n);
        for i in 0..n {
            let key = format!("unit_{i}");
            let dep = if i == 0 {
                "root".to_string()
            } else {
                format!("unit_{}", i - 1)
            };
            previous.push(AnalysisUnitMetadata {
                unit_key: key.clone(),
                unit_kind: AnalysisUnitKind::Function,
                body_hash: i as u64,
                body_hash2: i as u64,
                dependencies: vec![dep.clone()],
                origin: None,
            });
            current.push(AnalysisUnitMetadata {
                unit_key: key,
                unit_kind: AnalysisUnitKind::Function,
                body_hash: if i == 0 { 777 } else { i as u64 },
                body_hash2: if i == 0 { 777 } else { i as u64 },
                dependencies: vec![dep],
                origin: None,
            });
        }

        let report = compute_invalidation_report(&previous, &current);
        assert_eq!(report.changed_units, vec!["unit_0".to_string()]);
        assert_eq!(report.invalidated_units.len(), n);
    }

    #[test]
    fn invalidation_report_marks_dependents_of_changed_impl_method() {
        let previous = parse(
            "impl Point {\n    fn new: () :i64 {\n        return 1\n    }\n}\nfn main: () :i64 {\n    return Point::new()\n}\n",
        )
        .expect("parse previous");
        let current = parse(
            "impl Point {\n    fn new: () :i64 {\n        return 2\n    }\n}\nfn main: () :i64 {\n    return Point::new()\n}\n",
        )
        .expect("parse current");

        let report = compute_invalidation_report(
            &analysis_units_for_program(&previous),
            &analysis_units_for_program(&current),
        );

        assert!(report.changed_units.contains(&"impl::Point".to_string()));
        assert!(report.changed_units.contains(&"Point.new".to_string()));
        assert!(report.invalidated_units.contains(&"main".to_string()));
    }

    #[test]
    fn invalidation_report_matches_member_access_to_type_field_units() {
        let previous = Program {
            file_attributes: vec![],
            annotations: vec![],
            statements: vec![
                Statement::Type {
                    visibility: Visibility::Public,
                    name: "Point".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    parent: None,
                    fields: vec![Statement::Let {
                        name: "x".to_string(),
                        data_type: DataType::I64,
                        value: Some(Expression::Literal { lit: Literal::Int(1), line: 0, column: 0 }),
                        is_constant: false,
                        is_mutable: false,
                        is_static: false,
                        visibility: Visibility::Public,
                        name_line: 1,
                        name_column: 1,
                    }],
                },
                Statement::Function {
                    name: "main".to_string(),
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    params: vec![],
                    body: vec![Statement::Expression(Expression::MemberAccess {
                        target: Box::new(Expression::Identifier(Identifier {
                            name: "point".to_string(),
                            data_type: DataType::StructNamed("Point".to_string()),
                            line: 0,
                            column: 0,
                        })),
                        member: "x".to_string(),
                        data_type: DataType::Unknown,
                    })],
                    return_type: DataType::None,
                    visibility: Visibility::Public,
                    is_method: false,
                    attributes: Vec::new(),
                    name_line: 0,
                    name_column: 0,
                },
            ],
        };
        let mut current = previous.clone();
        let Statement::Type { fields, .. } = &mut current.statements[0] else {
            panic!("expected type");
        };
        let Statement::Let { value, .. } = &mut fields[0] else {
            panic!("expected field");
        };
        *value = Some(Expression::Literal { lit: Literal::Int(2), line: 0, column: 0 });

        let report = compute_invalidation_report(
            &analysis_units_for_program(&previous),
            &analysis_units_for_program(&current),
        );

        assert!(report.changed_units.contains(&"Point#x".to_string()));
        assert!(report.invalidated_units.contains(&"main".to_string()));
    }

    #[test]
    fn build_cache_distinguishes_test_and_normal_builds() {
        // `mire test` (test harness injected) and `mire run`/`build` share a
        // mode/import_mode/emit/persist profile, so the build cache key MUST
        // include test_mode — otherwise a harness binary is served to `owl run`
        // (tests print instead of the report). Regression for the
        // test-then-run build-cache collision.
        use crate::avens::{BuildMode, ImportMode, OptLevel};

        let root = std::env::temp_dir().join(format!("mire_cache_testmode_{}", now_epoch_ms()));
        setup_test_root(&root, &root.join("main.mire"));

        let mut cache =
            IncrementalCache::load_with_settings(&root.join("main.mire"), test_settings())
                .expect("load");

        let entry = |emit: bool, persist: bool| BuildCacheEntry {
            fingerprint: 42,
            mode: BuildMode::Debug,
            import_mode: ImportMode::default(),
            opt_level: OptLevel::O0,
            emit_binary: emit,
            persist_ir: persist,
            binary_path: PathBuf::from("test_bin"),
            ir_path: None,
            optimized_ir_path: None,
        };

        cache.store_build(&root.join("main.mire"), entry(true, false), true);
        cache.store_build(&root.join("main.mire"), entry(true, false), false);

        assert!(
            cache
                .build_entry(
                    &root.join("main.mire"),
                    BuildMode::Debug,
                    ImportMode::default(),
                    true,
                    false,
                    true
                )
                .is_some(),
            "test-mode build entry must be found"
        );
        let normal = cache
            .build_entry(
                &root.join("main.mire"),
                BuildMode::Debug,
                ImportMode::default(),
                true,
                false,
                false,
            )
            .expect("normal build entry must be found");
        assert_eq!(normal.binary_path, PathBuf::from("test_bin"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_caches_share_one_cache_dir_without_corruption() {
        // Mirrors `mire test -j N`: N threads each own an IncrementalCache but
        // all write to the same bin/.cache. Previously the shared WAL
        // `{ts}.wal` filename collided across threads and truncated a file to
        // invalid UTF-8 JSON, hard-failing the next load. The hardened WAL
        // (pid+seq names, create_new, tolerant replay) must make this safe.
        let root = std::env::temp_dir().join(format!("mire_cache_par_{}", now_epoch_ms()));
        fs::create_dir_all(&root).expect("temp dir");
        fs::write(
            root.join("owl.toml"),
            "[project]\nname = \"test\"\nversion = \"0.1.0\"\nentry = \"main.mire\"\n",
        )
        .expect("owl.toml");
        for i in 0..8 {
            fs::write(root.join(format!("mod{i}.mire")), "pub fn main: () {}\n").expect("source");
        }

        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for i in 0..8 {
                let root = root.clone();
                handles.push(scope.spawn(move || {
                    let source_path = root.join(format!("mod{i}.mire"));
                    let mut cache =
                        IncrementalCache::load_with_settings(&source_path, test_settings())
                            .expect("load");
                    for round in 0..16 {
                        cache
                            .store_file(
                                &source_path,
                                CachedParsedFile {
                                    hash: i as u64 + round,
                                    hash2: i as u64 + round,
                                    program: demo_program(&format!("main_{i}")),
                                    exports: vec!["main".to_string()],
                                    local_imports: Vec::new(),
                                },
                            )
                            .expect("store file");
                        cache
                            .store_analysis(
                                &source_path,
                                i as u64 + round,
                                0,
                                &demo_program(&format!("typed_{i}")),
                            )
                            .expect("store analysis");
                    }
                    cache.save().expect("save");
                    (i, source_path)
                }));
            }
            for handle in handles {
                handle.join().expect("thread joined");
            }
        });

        // Every writer's entries must survive a fresh load: no truncation, no
        // lost WAL records, no hard failures.
        let mut found = 0usize;
        for i in 0..8 {
            let source_path = root.join(format!("mod{i}.mire"));
            let mut cache =
                IncrementalCache::load_with_settings(&source_path, test_settings()).expect("reload");
            if cache.cached_analysis(&source_path, i as u64 + 15, 0).is_some() {
                found += 1;
            }
        }
        assert_eq!(found, 8, "all writers' final entries must roundtrip");

        let _ = fs::remove_dir_all(&root);
    }
}
