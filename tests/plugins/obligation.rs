#[cfg(test)]
mod tests {
    use decapod::core::obligation::{
        add_obligation, derive_obligation_status, get_obligation, initialize_obligation_db,
        list_obligations, validate_obligation_graph, ObligationStatus,
    };
    use decapod::core::store::{Store, StoreKind};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn test_store() -> (Store, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Store {
            root: PathBuf::from(temp_dir.path()),
            kind: StoreKind::Repo,
        };
        initialize_obligation_db(&store.root).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_add_obligation() {
        let (store, _temp) = test_store();
        let result = add_obligation(&store, "test-intent", "medium", "", "");
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_list_obligations() {
        let (store, _temp) = test_store();
        add_obligation(&store, "test-1", "medium", "", "").unwrap();
        add_obligation(&store, "test-2", "high", "", "").unwrap();

        let obligations = list_obligations(&store).unwrap();
        assert_eq!(obligations.len(), 2);
    }

    #[test]
    fn test_get_obligation() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "test-intent", "medium", "", "").unwrap();

        let obligation = get_obligation(&store, &id).unwrap();
        assert_eq!(obligation.id, id);
        assert_eq!(obligation.intent_ref, "test-intent");
        assert_eq!(obligation.status, ObligationStatus::Open);
    }

    #[test]
    fn test_derive_status_no_dependencies_no_proofs() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "test-intent", "medium", "", "").unwrap();

        let result = derive_obligation_status(&store, &id).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Open);
        assert!(!result.commit_present);
        assert!(result
            .validation_errors
            .iter()
            .any(|e| e.contains("STATE_COMMIT")));
    }

    #[test]
    fn test_derive_status_without_commit() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "test-intent", "medium", "", "").unwrap();

        // Without state commit, status should be Open
        let result = derive_obligation_status(&store, &id).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Open);
    }

    #[test]
    fn test_validate_graph_empty() {
        let (store, _temp) = test_store();
        let result = validate_obligation_graph(&store).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.total_nodes, 0);
    }

    #[test]
    fn test_validate_graph_with_obligations() {
        let (store, _temp) = test_store();
        add_obligation(&store, "test-1", "medium", "", "").unwrap();
        add_obligation(&store, "test-2", "high", "", "").unwrap();

        let result = validate_obligation_graph(&store).unwrap();
        // Graph is valid (no cycles), but obligations are not all satisfied
        assert!(!result.has_cycles);
        assert_eq!(result.total_nodes, 2);
    }

    #[test]
    fn test_dependency_chain() {
        let (store, _temp) = test_store();

        // Create obligation 1 (no dependencies)
        let id1 = add_obligation(&store, "test-1", "medium", "", "").unwrap();

        // Create obligation 2 that depends on 1
        let _id2 = add_obligation(&store, "test-2", "medium", &id1, "").unwrap();

        // Validate graph - should have valid structure (no cycles)
        let result = validate_obligation_graph(&store).unwrap();
        assert!(!result.has_cycles);
        assert_eq!(result.total_nodes, 2);
        assert_eq!(result.total_edges, 1);
    }
}
