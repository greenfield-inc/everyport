use everyport::protocol::{CleanUpReason, ServerStatus, Snapshot};

/// The shared fixture that the UI renders must stay valid protocol.
#[test]
fn fixture_snapshot_parses_as_protocol() {
    let json = include_str!("../../../packages/protocol/fixtures/snapshot.json");
    let snapshot: Snapshot = serde_json::from_str(json).expect("fixture matches protocol");
    let ports: Vec<u16> = snapshot.servers.iter().map(|s| s.port).collect();
    assert_eq!(ports, [3000, 3001, 5173, 6006, 8000]);
    let others: Vec<u16> = snapshot.other_ports.iter().map(|o| o.port).collect();
    assert_eq!(others, [631, 5432]);
    let leaking = &snapshot.servers[3];
    assert_eq!(leaking.status, ServerStatus::Attention);
    assert!(matches!(
        leaking.clean_up,
        Some(CleanUpReason::Leaking { .. })
    ));
}
