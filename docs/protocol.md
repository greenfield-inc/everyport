# The ppm protocol

`ppm` reports the servers on a machine as a stream of JSON events and takes JSON requests to act on them. Anything that can run a command or open a URL can use it: an editor extension, a status bar, a dashboard, or a workspace app.

There are two transports, with the same JSON:

- `ppm stdio`: events on stdout, requests on stdin. The desktop app runs it as a sidecar, and runs it on remote machines through `ssh`, `docker exec -i`, `kubectl exec -i --` or `wsl`.
- `ppm serve`: HTTP on loopback, with events as server-sent events.

The types are defined in [`crates/ppm-core/src/protocol.rs`](../crates/ppm-core/src/protocol.rs), and TypeScript types are generated from it into `@ppm/protocol`.

## Conventions

- Timestamps are Unix milliseconds.
- Memory is bytes.
- CPU values are floats, in percent of one core, so a busy server on an 8-core machine can report up to 800.0. `system.cpu_percent` is whole-machine CPU, from 0 to 100.
- Optional fields are always present, as `null` when empty. The tables below mark them "or `null`".
- "Clean up" is the list of servers `ppm` suggests stopping: those whose worktree was deleted, or that are idle, long-running or leaking.

## Framing

On stdio, every message is one line of JSON ending in `\n`: one event per line on stdout, and one request per line on stdin. Blank lines on stdin are ignored.

Over HTTP, events are server-sent events with one event per `data:` line, and a request is the body of a `POST`. See [ppm serve](#ppm-serve).

## Handshake

Every connection starts with `hello`, then a `snapshot` of the current state. After that, `ppm` sends:

- a `snapshot` whenever anything changes (it scans every 2 s by default)
- an `alert` when a server crosses a threshold
- a `result` for each request

A client needs no reply to `hello`. After a request, its `result` comes first, then any snapshot it caused. `ppm stdio` exits with status 0 when stdin closes, after answering every request it already read.

In this transcript, → is a line from `ppm` and ← a line to it.

```text
→ {"type":"hello","protocol":1,"ppm_version":"0.1.0","host":{"hostname":"devbox","os":"linux","arch":"x86_64","cores":8}}
→ {"type":"snapshot","taken_at":1790195040000,"system":{...},"servers":[...]}
← {"id":1,"method":"refresh"}
→ {"type":"result","id":1,"error":null}
→ {"type":"snapshot","taken_at":1790195041000,"system":{...},"servers":[...]}
```

## Events

Every event has a `type`.

### hello

The first event on every connection.

```json
{
  "type": "hello",
  "protocol": 1,
  "ppm_version": "0.1.0",
  "host": { "hostname": "devbox", "os": "linux", "arch": "x86_64", "cores": 8 }
}
```

`os` is `macos`, `linux` or `windows`. `arch` is the Rust target arch, such as `aarch64` or `x86_64`.

### snapshot

The full state. It's sent after `hello`, after every scan where something other than `taken_at` changed, and after every `refresh`. Replace your state with it; there are no diffs.

```json
{
  "type": "snapshot",
  "taken_at": 1790195040000,
  "system": {
    "memory_total": 17179869184,
    "memory_used": 12777527706,
    "memory_other_apps": 7516192768,
    "cpu_percent": 18.0
  },
  "servers": [
    {
      "port": 3000,
      "pid": 48213,
      "root": { "pid": 48198, "started_at": 1790183520000 },
      "process_name": "node",
      "addresses": ["127.0.0.1", "::1"],
      "cwd": "/Users/dev/conductor/workspaces/port-process-manager/providence",
      "cwd_exists": true,
      "command": "npm run dev",
      "launch_dir": "/Users/dev/conductor/workspaces/port-process-manager/providence",
      "started_at": 1790183520000,
      "project": {
        "name": "port-process-manager",
        "root": "/Users/dev/conductor/workspaces/port-process-manager/providence",
        "framework": "Next.js",
        "branch": "menubar-port-monitor",
        "worktree": "providence",
        "github": "greenfield-inc/port-process-manager",
        "vercel": {
          "project_id": "prj_ppm",
          "preview_url": "https://port-process-manager-git-menubar-port-monitor.vercel.app"
        }
      },
      "workspace": { "kind": "conductor", "name": "providence", "open_url": null },
      "agent": {
        "kind": "claude_code",
        "id": "68c8fda6-2f4e-4c1a-9a7b-1d2e3f4a5b6c",
        "title": "Dot-grid menu bar icon",
        "started_at": 1790183040000,
        "transcript_path": "/Users/dev/.claude/projects/providence/68c8fda6.jsonl",
        "directory": "/Users/dev/conductor/workspaces/port-process-manager/providence",
        "resume_command": "claude --resume 68c8fda6-2f4e-4c1a-9a7b-1d2e3f4a5b6c"
      },
      "processes": [
        { "proc": { "pid": 48198, "started_at": 1790183520000 }, "name": "npm run dev", "depth": 0, "memory": 60817408, "cpu_percent": 0.1 },
        { "proc": { "pid": 48213, "started_at": 1790183520900 }, "name": "next-server", "depth": 1, "memory": 1095761920, "cpu_percent": 9.8 }
      ],
      "memory": 1328545792,
      "cpu_percent": 12.0,
      "connections": 4,
      "history": [
        { "at": 1790194440000, "memory": 1267015352, "cpu_percent": 3.0 },
        { "at": 1790194470000, "memory": 1305507759, "cpu_percent": 4.0 }
      ],
      "last_active": 1790195000000,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": [
    { "port": 5432, "addresses": ["0.0.0.0"], "owner": "root", "process_name": "docker-proxy" }
  ]
}
```

`system`:

| Field | Meaning |
|---|---|
| `memory_total`, `memory_used` | The machine's physical memory and how much is in use |
| `memory_other_apps` | Memory used by everything that isn't a listed server |
| `cpu_percent` | Whole-machine CPU, 0 to 100 |

`servers` is sorted by port. Each server is a listening port and the process tree behind it:

| Field | Meaning |
|---|---|
| `port` | The listening TCP port |
| `pid` | The process that owns the socket |
| `root` | The topmost process of the server's tree, such as `npm run dev`. Pass it to `stop` and `restart`. |
| `process_name` | Name of the process that owns the socket |
| `addresses` | Bound addresses, such as `127.0.0.1` and `::1` |
| `cwd`, `cwd_exists` | The server's folder (or `null`), and whether it still exists. It's `false` once a worktree is deleted. |
| `command`, `launch_dir` | The root's command line and the folder it started in, or `null`. `restart` runs `command` in `launch_dir`. |
| `started_at` | When the root process started, or `null` |
| `project` | `name` (from `package.json`, the repo folder or the folder) is always set. `root`, `framework`, `branch`, `worktree`, `github` (`owner/repo`) and `vercel` are `null` when unknown. |
| `workspace` | The Conductor workspace, Pane worktree or git worktree the server runs in, or `null`. `kind` is `conductor`, `pane` or `git_worktree`. `open_url` opens it in its app, such as `pane://open?pane=<id>&panel=<id>`, or is `null`. |
| `agent` | The Claude Code or Codex session that started the server, or `null`. `kind` is `claude_code` or `codex`. `resume_command` resumes it in `directory`. |
| `processes` | The whole tree, depth first and root first. `depth` is 0 for the root. |
| `memory`, `cpu_percent` | Sums over `processes` |
| `connections` | Open connections to the port |
| `history` | Samples covering up to the last 10 minutes, oldest first. Don't assume a fixed spacing. |
| `last_active` | Last time the server had connections or used CPU |
| `protected` | A process in its tree is on the protected list, such as `postgres`. Clean up never suggests it, and `stop` and `restart` need `confirm_protected`. |
| `status` | `running`, `attention` (over the memory threshold or leaking) or `idle` (idle for over an hour, or its folder is gone) |
| `clean_up` | Why Clean up suggests stopping it, or `null` |

`clean_up` is one of:

```json
{ "kind": "worktree_deleted" }
{ "kind": "idle", "seconds": 18000 }
{ "kind": "long_running", "seconds": 259200 }
{ "kind": "leaking", "bytes": 1191182336 }
```

`other_ports` lists the ports that processes of other users or the system hold, such as a Docker-published port or a system database. `ppm` can't inspect those processes, so these ports have no tree and no actions. The list is sorted by port and covers the configured port range. A port in `servers` never appears here.

| Field | Meaning |
|---|---|
| `port` | The listening TCP port |
| `addresses` | Bound addresses, such as `0.0.0.0` and `::` |
| `owner` | The user the process runs as, such as `root`, or `null` when the OS doesn't say |
| `process_name` | Name of the process that owns the socket, or `null` when the OS doesn't say. Linux doesn't tell a normal user which process holds another user's socket. |

On macOS and Linux, when `ppm` runs as root, every port is a server and `other_ports` is empty. On macOS it's empty for other users too: macOS lists other users' sockets only to its own tools, such as `netstat`.

### alert

A server crossed a threshold. `ppm` sends it once, and again only after the server recovers and crosses it again. Clients decide whether and how to notify.

```json
{ "type": "alert", "port": 6006, "kind": "leaking", "memory": 3017089024 }
```

`kind` is `over_threshold` (memory above [`alert_memory`](#configure)) or `leaking` (grew by [`leak_growth`](#configure) over the history window).

### result

The answer to the request with the same `id`. `error` is `null` on success, or a message for the user.

```json
{ "type": "result", "id": 7, "error": null }
{ "type": "result", "id": 8, "error": "pid 48198 has exited or now belongs to another process" }
```

A line that isn't a valid request still gets a `result` with an error. Its `id` is the request's `id` when one can be read, and `0` otherwise, so avoid using `0` as an id.

## Requests

A request has a numeric `id` that you choose, a `method`, and `params` for the methods that take them. Every field of `params` is required. Requests run one at a time, in order.

Use ids from 1 up to 2^53, so JavaScript clients keep them exact. `ppm` only echoes them back, so they need to be unique only among your requests in flight.

### refresh

Scan now and send a fresh snapshot, even if nothing changed.

```json
{ "id": 1, "method": "refresh" }
```

### stop

Stop a server's process tree, deepest processes first. `ppm` asks each process to quit, and kills whatever is left after 3 s. With `force: true`, it kills at once. On macOS and Linux, asking is SIGTERM and killing is SIGKILL. On Windows, asking sends Ctrl+C to the server's console when only the server and the shells that launched it are on that console. Otherwise it closes the process's windows, or terminates a process that has none. Killing terminates it. `port` is the server's port, and `root` must be the server's `root` from the snapshot. `ppm` checks every process's start time first, so it never signals a process whose pid was reused.

A server is protected when any process in its tree is on the `protected` list, such as `postgres`. `ppm` refuses to stop it unless `confirm_protected` is `true`, and the error `result` names the protected process, such as `postgres :5432 is protected; send confirm_protected to stop it anyway`. `confirm_protected` defaults to `false`. Clients ask the user before sending `true`.

```json
{ "id": 2, "method": "stop", "params": { "port": 3000, "root": { "pid": 48198, "started_at": 1790183520000 }, "force": false, "confirm_protected": false } }
```

The result arrives as soon as the processes are asked to quit. The server leaves the snapshot once its tree is gone.

### restart

Stop the server, then run its `command` again in its `launch_dir`, detached from `ppm`. The result arrives as soon as the old tree is asked to quit. Once that tree is gone and `port` is free, `ppm` starts the command, and the new server shows up in a later snapshot.

A protected server needs `confirm_protected: true`, as for `stop`. An error `result` covers what `ppm` can check up front: the server is protected, the process changed, or its command or folder can't be read. A failure after that gets no second `result`. The server just doesn't come back on `port` in the snapshots over the next 10 s or so. The new server's output is in `port-process-manager/port-<port>.log` in the system temp folder (`$TMPDIR` or `%TEMP%`), and a failure to start it is one line on `ppm`'s stderr.

```json
{ "id": 3, "method": "restart", "params": { "port": 3000, "root": { "pid": 48198, "started_at": 1790183520000 }, "confirm_protected": false } }
```

### configure

Replace all scanner settings. Send every field; there are no partial updates. On stdio, they apply to that connection's scanner. Over HTTP, one scanner serves every client, so they apply to all of them.

```json
{
  "id": 4,
  "method": "configure",
  "params": {
    "min_port": 3000,
    "max_port": 65535,
    "interval_ms": 2000,
    "alert_memory": 2147483648,
    "leak_growth": 524288000,
    "idle_after_secs": 14400,
    "long_running_after_secs": 259200,
    "protected": ["postgres", "redis-server", "mongod", "mysqld", "mysql"]
  }
}
```

These are the defaults.

| Field | Meaning |
|---|---|
| `min_port`, `max_port` | Ports to report |
| `interval_ms` | Time between scans |
| `alert_memory` | Memory above which a server needs attention and raises an alert |
| `leak_growth` | Growth over the history window that counts as leaking |
| `idle_after_secs` | Idle time after which Clean up suggests a server |
| `long_running_after_secs` | Uptime after which Clean up suggests a server |
| `protected` | Process names that protect a server: Clean up never suggests it, and `stop` and `restart` need `confirm_protected` |

## Versioning

`hello.protocol` is `1`. It goes up only when a change would break existing clients, such as removing or renaming a field or changing its meaning. New fields, event types and methods are added without a bump, so:

- ignore fields you don't know
- ignore events whose `type` you don't know
- expect an error `result` from an older `ppm` for a method it doesn't know

If `hello.protocol` is higher than the version you support, ask the user to update the client, and show `hello.ppm_version` in the message.

## Other machines

On another machine, run the same `ppm stdio` through a command prefix, such as `ssh devbox ppm stdio` or `docker exec -i box ppm stdio`. The protocol is the same.

To open a server from a machine whose prefix can't forward ports, such as `docker exec -i`, run `ppm connect <port>` through the prefix. It connects to `localhost:<port>` on that machine and pipes the connection to its stdin and stdout, so a client can relay one TCP connection per `ppm connect`. It exits when the server closes the connection, and fails with status 1 when nothing answers on the port. `ppm connect --check <port>` only connects and exits, so a client can check the port before it listens locally. ssh and kubectl forward ports themselves, so they don't need it.

## ppm serve

```bash
ppm serve                                   # listens on 127.0.0.1:7767
ppm serve --listen 127.0.0.1:8080
ppm serve --url https://devbox.tail1234.ts.net
ppm serve --allow-origin https://dash.example.com
```

`ppm serve` listens only on loopback, and refuses any other address. Put a tunnel or proxy you trust in front of it, such as `tailscale serve --bg http://127.0.0.1:7767`.

### Token and connection code

Every request needs `Authorization: Bearer <token>`. The token is created on first run and kept in `serve-token` in the ppm config folder (`~/Library/Application Support/port-process-manager` on macOS, `%APPDATA%\port-process-manager` on Windows, `~/.config/port-process-manager` on Linux), readable only by your user. To make a new token, delete the file and restart `ppm serve`.

On start, `ppm serve` prints a connection code:

```text
ppm://eyJ0b2tlbiI6Ii4uLiIsInVybCI6Imh0dHA6Ly8xMjcuMC4wLjE6Nzc2NyJ9
```

It's `ppm://` followed by the unpadded base64url encoding of a JSON object with the URL and the token:

```json
{ "url": "http://127.0.0.1:7767", "token": "..." }
```

`url` is the listen address unless you pass `--url`, which you should when clients reach the server through a tunnel. The code contains the token, so share it only with people who may control your servers.

### GET /events

A server-sent event stream. Each event is one `data:` line with the event's JSON, followed by a blank line. It starts with `hello` and a `snapshot`, like stdio. A `: ping` comment arrives every 15 s so proxies keep the stream open. There are no `id:` or `event:` fields and no resume: after a reconnect, you get `hello` and a full `snapshot` again.

```bash
curl -N -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7767/events
```

```text
data: {"type":"hello","protocol":1,"ppm_version":"0.1.0","host":{...}}

data: {"type":"snapshot","taken_at":1790195040000,"system":{...},"servers":[...]}

: ping

```

Results come only in the `/call` response.

### POST /call

Runs one request and returns its `result` event as the response body. The body is read as JSON whatever its `Content-Type`.

```bash
curl -H "Authorization: Bearer $TOKEN" -d '{"id":1,"method":"refresh"}' http://127.0.0.1:7767/call
```

```json
{"type":"result","id":1,"error":null}
```

Snapshot changes the request causes arrive on `/events`.

### Status codes

| Status | When |
|---|---|
| `200` | `/events` stream, or `/call` ran the request. A failed request is still `200`, with `error` set. |
| `400` | The body isn't a valid request (the body is a `result` with the error), or the HTTP request is malformed |
| `401` | The token is missing or wrong |
| `404`, `405` | Unknown path, or the wrong method for it |

### Browsers

By default, responses carry no CORS headers, so a web page can't read them, even with a leaked connection code. To use the API from a page, allow its origin with `--allow-origin`, once per origin:

```bash
ppm serve --allow-origin https://dash.example.com --allow-origin http://localhost:5173
```

Responses to a request from an allowed origin carry `Access-Control-Allow-Origin` with that origin, and `OPTIONS` preflight requests from it are answered without a token. Every other request still needs the token. `EventSource` can't send headers, so read `/events` with `fetch` and a stream reader.
