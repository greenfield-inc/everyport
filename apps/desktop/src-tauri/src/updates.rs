//! Keeps the popover's `machines` updates small. Each update has a `seq`,
//! one more than the last, so the page can tell when it missed one. Each
//! server is `{"full": server}` when the page hasn't seen it or a new process
//! took its port, and otherwise `{"patch": ...}` with its `port` and the fields
//! that changed. In a patch, `history` carries only the samples from the
//! page's newest one on, and `processes` lists each process by its `proc`,
//! with a new one in full and a known one with only the fields that changed.
//! updates.ts applies them.

use std::collections::HashMap;

use ppm_client::protocol::ProcRef;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::machines::Machine;

/// A server as the engine tracks it: by machine, port and root process.
type Key = (String, u16, ProcRef);

/// What the page has: each server as it last got it, and the last `seq`.
#[derive(Default)]
pub struct Sent {
    servers: HashMap<Key, Map<String, Value>>,
    seq: u64,
}

/// An update, and what the page has once it's delivered.
pub struct Update {
    pub payload: Value,
    servers: HashMap<Key, Map<String, Value>>,
}

/// Every machine in full, and the `seq` the next update follows.
#[derive(Serialize)]
pub struct Base {
    pub seq: u64,
    pub machines: Vec<Machine>,
}

impl Sent {
    /// The page starts over from `machines`, so the next update sends every
    /// server in full.
    pub fn base(&mut self, machines: Vec<Machine>) -> Base {
        self.servers.clear();
        Base {
            seq: self.seq,
            machines,
        }
    }

    /// The machines as an update to what the page has.
    pub fn update<'a>(&self, machines: impl IntoIterator<Item = &'a Machine>) -> Update {
        let mut sent = HashMap::new();
        let machines: Vec<Value> = machines
            .into_iter()
            .map(|machine| {
                let mut value = serde_json::to_value(machine).expect("machines serialize");
                let (Some(snapshot), Some(servers)) = (
                    machine.snapshot.as_ref(),
                    value
                        .pointer_mut("/snapshot/servers")
                        .and_then(Value::as_array_mut),
                ) else {
                    return value;
                };
                for (server, json) in snapshot.servers.iter().zip(servers.iter_mut()) {
                    let key = (machine.id.clone(), server.port, server.root);
                    let Value::Object(full) = json.take() else {
                        unreachable!("a server serializes to an object")
                    };
                    *json = match self.servers.get(&key) {
                        Some(old) => json!({ "patch": changes(old, &full, "port") }),
                        None => json!({ "full": full }),
                    };
                    sent.insert(key, full);
                }
                value
            })
            .collect();
        Update {
            payload: json!({ "seq": self.seq + 1, "machines": machines }),
            servers: sent,
        }
    }

    /// The page got `update`. Servers that stopped, or whose machine
    /// disconnected, go in full if they come back.
    pub fn delivered(&mut self, update: Update) {
        self.servers = update.servers;
        self.seq += 1;
    }
}

/// The `key` field, the fields that differ, the new end of `history`, and
/// `processes` as changes too.
fn changes(old: &Map<String, Value>, new: &Map<String, Value>, key: &str) -> Map<String, Value> {
    new.iter()
        .filter_map(|(field, value)| {
            let change = match field.as_str() {
                _ if field == key => value.clone(),
                _ if old.get(field) == Some(value) => return None,
                "history" => Value::Array(tail(&old[field], value)),
                "processes" => Value::Array(processes(&old[field], value)),
                _ => value.clone(),
            };
            Some((field.clone(), change))
        })
        .collect()
}

/// Each process, in full if it's new, otherwise its `proc` and what changed.
fn processes(old: &Value, new: &Value) -> Vec<Value> {
    let id = |p: &Value| (p["proc"]["pid"].as_u64(), p["proc"]["started_at"].as_u64());
    let old: HashMap<_, &Map<String, Value>> = old
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((id(p), p.as_object()?)))
        .collect();
    new.as_array()
        .into_iter()
        .flatten()
        .map(
            |process| match (old.get(&id(process)), process.as_object()) {
                (Some(before), Some(now)) => Value::Object(changes(before, now, "proc")),
                _ => process.clone(),
            },
        )
        .collect()
}

/// The samples from the old history's last one on, since the engine keeps
/// updating its newest sample until the next one starts.
fn tail(old: &Value, new: &Value) -> Vec<Value> {
    let at = |sample: &Value| sample["at"].as_u64().unwrap_or(0);
    let from = old.as_array().and_then(|h| h.last()).map_or(0, at);
    new.as_array()
        .into_iter()
        .flatten()
        .filter(|sample| at(sample) >= from)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machines::MachineState;
    use ppm_client::protocol::{Sample, Snapshot};

    fn fixture() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../../packages/protocol/fixtures/snapshot.json"
        ))
        .unwrap()
    }

    fn machine(snapshot: Snapshot) -> Machine {
        Machine {
            id: "local".into(),
            label: "mac".into(),
            host: None,
            state: MachineState::Connected,
            error: None,
            install: None,
            snapshot: Some(snapshot),
        }
    }

    fn servers(update: &Update) -> &Vec<Value> {
        update.payload["machines"][0]["snapshot"]["servers"]
            .as_array()
            .unwrap()
    }

    /// Sends `snapshot` and marks it delivered.
    fn deliver(sent: &mut Sent, snapshot: &Snapshot) -> Value {
        let update = sent.update(&[machine(snapshot.clone())]);
        let payload = update.payload.clone();
        sent.delivered(update);
        payload
    }

    #[test]
    fn sends_a_server_in_full_once_then_what_changed() {
        let mut sent = Sent::default();
        let first = fixture();
        let update = sent.update(&[machine(first.clone())]);
        assert_eq!(update.payload["seq"], 1);
        assert_eq!(servers(&update)[0], json!({ "full": first.servers[0] }));
        sent.delivered(update);

        // The next scan: next-server under :3000 grew by 1 MB, its newest
        // sample took the new memory, and a new sample started 10 s later.
        let mut next = first.clone();
        next.taken_at += 10_000;
        let server = &mut next.servers[0];
        server.memory += 1_000_000;
        server.processes[1].memory += 1_000_000;
        let newest = *server.history.last().unwrap();
        server.history.last_mut().unwrap().memory = server.memory;
        server.history.push(Sample {
            at: newest.at + 10_000,
            ..newest
        });
        let update = sent.update(&[machine(next.clone())]);

        let server = &next.servers[0];
        let [npm, next_server, turbopack] = &server.processes[..] else {
            panic!("the fixture's :3000 runs three processes")
        };
        let n = server.history.len();
        assert_eq!(update.payload["seq"], 2);
        assert_eq!(
            servers(&update)[0],
            json!({ "patch": {
                "port": 3000,
                "memory": server.memory,
                "processes": [
                    { "proc": npm.proc },
                    { "proc": next_server.proc, "memory": next_server.memory },
                    { "proc": turbopack.proc },
                ],
                "history": server.history[n - 2..],
            }})
        );
        // Unchanged servers carry only their port.
        assert_eq!(
            servers(&update)[1],
            json!({ "patch": { "port": next.servers[1].port } })
        );
        assert_eq!(
            update.payload["machines"][0]["snapshot"]["taken_at"],
            json!(next.taken_at)
        );
    }

    #[test]
    fn an_update_that_never_arrived_is_sent_again() {
        let mut sent = Sent::default();
        let first = fixture();
        deliver(&mut sent, &first);

        // :3001 moves to another branch, but the emit fails.
        let mut checkout = first.clone();
        checkout.servers[1].project.branch = Some("fix/login".into());
        let lost = sent.update(&[machine(checkout.clone())]);
        assert_eq!(lost.payload["seq"], 2);

        // The next scan repeats the change under the same seq.
        let mut next = checkout.clone();
        next.servers[1].connections += 1;
        let payload = deliver(&mut sent, &next);
        assert_eq!(payload["seq"], 2);
        assert_eq!(
            payload["machines"][0]["snapshot"]["servers"][1],
            json!({ "patch": {
                "port": 3001,
                "project": next.servers[1].project,
                "connections": next.servers[1].connections,
            }})
        );
    }

    #[test]
    fn a_new_process_on_a_port_or_a_page_that_starts_over_gets_it_in_full() {
        let mut sent = Sent::default();
        deliver(&mut sent, &fixture());

        let mut restarted = fixture();
        restarted.servers[0].root.pid += 1;
        let payload = deliver(&mut sent, &restarted);
        let servers = &payload["machines"][0]["snapshot"]["servers"];
        assert_eq!(servers[0], json!({ "full": restarted.servers[0] }));
        assert_eq!(servers[1]["patch"]["port"], 3001);

        let base = sent.base(vec![machine(restarted.clone())]);
        assert_eq!(base.seq, 2);
        let payload = deliver(&mut sent, &restarted);
        assert_eq!(payload["seq"], 3);
        assert_eq!(
            payload["machines"][0]["snapshot"]["servers"][1],
            json!({ "full": restarted.servers[1] })
        );
    }
}
