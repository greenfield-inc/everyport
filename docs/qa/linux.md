# Linux QA

Tested revision: `48c09a7` (main after rebasing during QA), on 2026-09-29.
This is a source-build QA run, not verification of a published release.

The host is an Apple Silicon Mac running Docker through Colima. All listeners and
all stop, restart and clean tests run in disposable containers created for this
run, on ports 39101–39190. No existing Mac servers were changed. Remote CLI
settings use a separate temporary home directory.

## Findings

1. **P2: plain listing says nothing is listening when other users own every
   listener.** Run `su qa -s /bin/sh -c 'ppm list'` while the root fixtures are
   listening. It prints `Nothing listening on ports 3000-65535.` The same user's
   `ppm list --json` includes those ports in `other_ports`. Route to the CLI lane:
   render the other-port rows and consider both collections before showing an
   empty state. This affects local and remote plain listing through the shared
   formatter. The local behavior is verified; the remote consequence is inferred
   from that shared code.
2. **P2: the README promises process names for other users' ports, but Linux
   reports `null`.** The normal user's JSON correctly lists the five root-owned
   ports and owner `root`, with `process_name: null` on every entry. Linux cannot
   map the socket inode to another user's unreadable `/proc/<pid>/fd` directory.
   Route to the platform/docs lanes to document this permission limit or provide
   a reliable supported attribution mechanism. Do not guess a process from its
   port number.

The initial run on `a9dc3e6` also found that listeners launched from `/` were
missing and could not be stopped. PR #16 merged during QA. After rebasing and
rebuilding on `48c09a7`, all three distros list :39103 and successfully force-stop
it. That finding is resolved upstream.

No product code changed in this lane. These findings cross presentation and
permission policy; this report leaves those decisions to their owning lanes
rather than silently changing established behavior. The report records the
findings, with repro commands and observed output below.

## Claim matrix

All results below are **ARM64**. Every corresponding x86_64 runtime check is
**blocked** by Docker's `exec format error`.

| Claim / check | Ubuntu 24.04 | Debian 12 | Alpine 3.20 |
|---|---|---|---|
| Self-contained musl binary executes | Pass | Pass | Pass |
| Shell installer, default user path and matching SHA-256 | Pass | Pass | Pass |
| Packed npm wrapper via `npx`, fresh cache | Pass | Pass | Pass |
| Built PyPI wheel via `uvx --from`, separate fresh cache | Pass | Pass | Pass; uv warning noted below |
| All three installers reject corrupted payload | Pass | Pass | Pass |
| All three installers work as normal user | Pass | Pass | Pass |
| Python / Node project, framework and branch | Pass | Pass | Pass |
| Listening server launched from `/` | Pass after #16 | Pass after #16 | Pass after #16 |
| `list --json` and `watch --jsonl`, parseable snapshots | Pass | Pass | Pass |
| Other user's port and owner in JSON | Pass | Pass | Pass |
| Other user's port in plain `list` | Fail F1 | Fail F1 | Fail F1 |
| Other user's process name | Fail F2 | Fail F2 | Fail F2 |
| Own server details without admin rights | Pass | Pass | Pass |
| Two-process tree, memory and sampled CPU | Pass | Pass | Pass |
| `stop`, including normal-user `--force` | Pass | Pass | Pass |
| `restart`, new process and responding endpoint | Pass | Pass | Pass |
| `clean --yes` removes deleted-directory fixture | Pass | Pass | Pass |
| TUI list, detail, help, back and quit | Pass | Pass | Pass |
| Mac remote add, missing-install prompt, install, list, remove | Pass | Pass | Pass |
| Mac remote open, forwarded HTTP body | Pass | Pass | Pass |
| Serve rejects non-loopback bind | Pass | Pass | Pass |
| Serve missing/wrong token returns 401 | Pass | Pass | Pass |
| Authenticated SSE hello and snapshot | Pass | Pass | Pass |
| Authenticated HTTP refresh call | Pass | Pass | Pass |
| Token permissions `0600` | Pass | Pass | Pass |
| Stdio hello, configure reply, snapshots, EOF exit | Pass | Pass | Pass |
| Sustained memory threshold alert | Pass | Pass | Pass |
| `doctor`, including NVIDIA fallback text | Pass | Pass | Pass |

The tree fixture forks a worker that allocates 32 MiB and keeps one CPU busy.
Both processes appear, aggregate memory exceeds that known allocation, and CPU
is positive. This checks aggregation behavior, not a calibrated RSS/CPU accuracy
benchmark. The stdio check uses port 39102, a 1-byte memory threshold and a 200 ms
scan interval for 33 seconds. It receives one `over_threshold` alert and an
`attention` snapshot. It does not establish leak detection or ten-minute retention.

Alpine's uv-generated launcher prints `realpath: --: No such file or directory`
before both successful runs and checksum failures. The wrapper still downloads,
validates and executes correctly. This warning was not isolated to product code
and is not counted as a product failure.

## Coverage limits

- Ubuntu 24.04, Debian 12 and Alpine 3.20: native ARM64 containers.
- x86_64: each distro and `rust:1-alpine` fails before startup with
  `exec /bin/sh: exec format error` (Debian reports `/usr/bin/sh`). This Docker
  installation has no working x86_64 emulation. No x86_64 runtime pass is claimed.
- Protection PR #13 was open at the tested revision. Its new force/protection
  behavior is not part of this report.
- Desktop bundles, tray behavior, GUI themes, signed installers, SSH, Kubernetes,
  WSL, Tailscale, Vercel and agent-session integration require separate coverage.
- Long-duration history, leak alerts, automatic cleanup and the native desktop
  performance budgets are not established by these short CLI checks.

## Reproduction setup

Build the CLI from the tested revision inside `rust:1-alpine`, with `musl-dev`,
`pkgconfig`, `openssl-dev` and `openssl-libs-static` installed. The workspace copy
must include `apps/desktop/src-tauri` even when building only the CLI.

The first release link was killed with signal 9. Disabling LTO only on the final
crate could not link the existing bitcode dependencies. The full-profile retry uses:

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=false \
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo build --release -p port-process-manager
```

This changes build optimization settings, not product source. Install the distro's
Python, Node, npm, git, curl and tmux packages; install `uv` in a Python virtual
environment. Create a normal user named `qa`.

Fixtures:

| Port | Launch directory | Process and metadata |
|---|---|---|
| 39101 | `/work/python` | `python3 -m http.server 39101`, pyproject name `qa-python` |
| 39102 | `/work/node` | `node server.js`, package name `qa-node`, Express manifest dependency, git branch `qa-linux` |
| 39103 | `/` | `python3 -m http.server 39103` |
| 39104 | `/work/deleted` | `python3 -m http.server 39104`, remove its empty working directory after launch |
| 39109 | temporary | `ppm serve --listen 127.0.0.1:39109` |
| 39190 | `/` | local HTTP release server, `--directory /qa/artifacts` |

The Node fixture uses the standard HTTP module, returning `qa-node`. Its manifest
includes Express to check manifest-based framework labeling; it does not test
Express itself. The Git repository has an unborn `qa-linux` branch.

The release folder holds `ppm-aarch64-unknown-linux-musl` and `SHA256SUMS` generated
with `sha256sum`. Copy those checksums into both wrapper source trees before
`npm pack` and `uv build --wheel`. Set `PPM_DOWNLOAD_URL` to the local release URL.
Use separate empty caches for npm and Python so both must download and verify.
For rejection tests, serve the literal `corrupt\n` under the binary's asset name
alongside the original checksum file, again with empty caches.


Remote tests set `HOME` to a fresh temporary Mac directory and preserve Docker's
endpoint through `DOCKER_HOST` (otherwise the temporary home loses the Colima
context). `PPM_BINARY_DIR` points to the same musl artifact. Each target's installed
binary is removed before testing the prompt; `y` is entered through a real PTY.
After `ppm --on <name> open 39102`, an HTTP request to the printed local URL must
return `qa-node`. Each forwarding process is stopped with SIGINT by its own PID.

## Evidence

Runtime versions: Ubuntu Node 18.19.1 / Python 3.12.3; Debian Node 18.20.4 /
Python 3.11.2; Alpine Node 20.15.1 / Python 3.12.13. All use uv 0.12.20.
The build container uses Rust 1.98.1. Artifact SHA-256:

```text
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287
```

Below are captured transcripts. Large snapshot JSON is projected to the fields
being checked; SSE snapshot bodies are shortened. Command prompts show the argument sequence; shell `-c` payloads need quoting
when copied. Host binary paths in the remote transcript are shortened to `ppm`. TUI captures are plain text from tmux at
100 × 32 or 110 × 32, not claims about desktop rendering.

<details>
<summary>Ubuntu: installs, listing, actions, serve and doctor</summary>

```text
$ sh /src/scripts/install.sh
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190
Installed ppm 0.1.0 to /root/.local/bin/ppm
Add /root/.local/bin to your PATH, for example: export PATH="/root/.local/bin:$PATH"
exit: 0
$ /root/.local/bin/ppm --version
ppm 0.1.0
exit: 0
$ sha256sum /root/.local/bin/ppm /qa/artifacts/ppm-aarch64-unknown-linux-musl
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287  /root/.local/bin/ppm
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287  /qa/artifacts/ppm-aarch64-unknown-linux-musl
exit: 0
$ npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
exit: 0
$ uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Installed 1 package in 1ms
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
exit: 0
$ /root/.local/bin/ppm list
PORT    NAME               BRANCH    MEMORY  CPU   UP  SESSION
:39101  qa-python                     11 MB   0%   1m
:39102  qa-node            qa-linux   42 MB   0%   6m
:39103  /                             11 MB   0%  10m
:39104  deleted (deleted)             11 MB   0%   1m
:39190  /                             11 MB   0%   8m
exit: 0
$ /root/.local/bin/ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39101,
      "pid": 10247,
      "root": {
        "pid": 10247,
        "started_at": 1790675846410
      },
      "cwd": "/work/python",
      "cwd_exists": true,
      "project": {
        "name": "qa-python",
        "root": "/work/python",
        "framework": "Python",
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11159552,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39102,
      "pid": 9708,
      "root": {
        "pid": 9708,
        "started_at": 1790675482990
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 44077056,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39103,
      "pid": 9522,
      "root": {
        "pid": 9522,
        "started_at": 1790675277190
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11176960,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39104,
      "pid": 10249,
      "root": {
        "pid": 10249,
        "started_at": 1790675846420
      },
      "cwd": "/work/deleted (deleted)",
      "cwd_exists": false,
      "project": {
        "name": "deleted (deleted)",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11151360,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "idle",
      "clean_up": {
        "kind": "worktree_deleted"
      }
    },
    {
      "port": 39190,
      "pid": 9542,
      "root": {
        "pid": 9542,
        "started_at": 1790675391020
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11694080,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
watch snapshots: 2
watch ports: [39101, 39102, 39103, 39104, 39190]
watch ports: [39101, 39102, 39103, 39104, 39190]
$ cp /root/.local/bin/ppm /usr/local/bin/ppm
exit: 0
$ su qa -s /bin/sh -c ppm list --json
[snapshot JSON projection]
{
  "servers": [],
  "other_ports": [
    {
      "port": 39101,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39102,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39103,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39104,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39190,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    }
  ]
}
exit: 0
$ /root/.local/bin/ppm doctor
ppm 0.1.0 · protocol 1 · Linux aarch64 · 2 cores
✓ Listening ports: 5 found
✓ Processes: 10 found
✓ Folder and command of your own processes
✓ Memory and CPU of your own processes
✓ Settings folder: /root/.config/port-process-manager
  Desktop app window blank on NVIDIA? Start it with WEBKIT_DISABLE_DMABUF_RENDERER=1.
exit: 0
$ /root/.local/bin/ppm serve --listen 0.0.0.0:39109
ppm: 0.0.0.0:39109 is not a loopback address; ppm serve listens only on loopback, so put a tunnel or proxy in front of it
exit: 1
GET /events missing token: 401
GET /events wrong token: 401
GET /events valid token: 200 text/event-stream
data: {"type":"hello","protocol":1,"ppm_version":"0.1.0","host":{"hostname":"0e89cad1a712","os":"linux","arch":"aarch64","cores":2}}

data: {"type":"snapshot", ...} [body omitted; listing checked separately]

$ sh -c cat /proc/net/tcp | awk '$2 ~ /:98C5$/ {print $2}'
0100007F:98C5
0100007F:98C5
0100007F:98C5
0100007F:98C5
exit: 0
[Initial TUI transcript omitted; dedicated captures below.]
$ /root/.local/bin/ppm restart 39102
Restarted :39102 with node server.js
exit: 0
$ curl -fsS http://127.0.0.1:39102
qa-node
exit: 0
$ /root/.local/bin/ppm stop 39101
Stopped qa-python :39101
exit: 0
$ /root/.local/bin/ppm stop 39103 --force
Stopped / :39103, killed
exit: 0
$ /root/.local/bin/ppm clean --yes
:39104 deleted (deleted)           12 MB   Worktree deleted
Stopped 1 server, freeing 12 MB.
exit: 0
$ /root/.local/bin/ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39102,
      "pid": 10373,
      "root": {
        "pid": 10373,
        "started_at": 1790675892970
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 46900224,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39190,
      "pid": 9542,
      "root": {
        "pid": 9542,
        "started_at": 1790675391020
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 13760512,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
$ sh /src/scripts/install.sh
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190/bad
ppm install: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
$ npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
$ uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Installed 1 package in 1ms
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
```

</details>

<details>
<summary>Ubuntu: normal-user installs, stdio, tree and HTTP call</summary>

```text
$ docker exec ppm-w3-ubuntu-arm64 su qa -s /bin/sh -c export PPM_DOWNLOAD_URL=http://127.0.0.1:39190; sh /src/scripts/install.sh && npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version && XDG_CACHE_HOME=/home/qa/.cache/python-qa uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190
Installed ppm 0.1.0 to /home/qa/.local/bin/ppm
Add /home/qa/.local/bin to your PATH, for example: export PATH="/home/qa/.local/bin:$PATH"
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
Installed 1 package in 1ms
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0

$ docker exec -e QA_DISTRO=ubuntu ppm-w3-ubuntu-arm64 python3 /qa/extra.py
Nothing listening on ports 3000-65535.
$ su qa -s /bin/sh -c "ppm list"
$ ppm stdio (configure port 39102, 1-byte memory alert, 200ms interval; observe 33s)
handshake: {'type': 'hello', 'protocol': 1, 'ppm_version': '0.1.0', 'host': {'hostname': '0e89cad1a712', 'os': 'linux', 'arch': 'aarch64', 'cores': 2}}
configure results: [{'type': 'result', 'id': 1, 'error': None}]
alerts: [{'type': 'alert', 'port': 39102, 'kind': 'over_threshold', 'memory': 45121536}]
snapshots: 158
final server summary: [{'port': 39102, 'status': 'attention', 'memory': 45121536, 'cpu_percent': 0.0, 'history_samples': 4}]
stdio exit: 0

{
  "command": "ppm list --json (normal user, forked 32 MiB worker on 39107)",
  "server": {
    "port": 39107,
    "pid": 10611,
    "root": {
      "pid": 10611,
      "started_at": 1790675961640
    },
    "process_name": "python3",
    "addresses": [
      "0.0.0.0"
    ],
    "cwd": "/work/python",
    "cwd_exists": true,
    "command": "python3 tree.py",
    "launch_dir": "/work/python",
    "started_at": 1790675961640,
    "project": {
      "name": "qa-python",
      "root": "/work/python",
      "framework": "Python",
      "branch": null,
      "worktree": null,
      "github": null,
      "vercel": null
    },
    "workspace": null,
    "agent": null,
    "processes": [
      {
        "proc": {
          "pid": 10611,
          "started_at": 1790675961640
        },
        "name": "python3 tree.py",
        "depth": 0,
        "memory": 8403968,
        "cpu_percent": 0.0
      },
      {
        "proc": {
          "pid": 10617,
          "started_at": 1790675961680
        },
        "name": "python3 tree.py",
        "depth": 1,
        "memory": 39819264,
        "cpu_percent": 75.098816
      }
    ],
    "memory": 48223232,
    "cpu_percent": 75.098816,
    "connections": 0,
    "history": [
      {
        "at": 1790675964381,
        "memory": 48223232,
        "cpu_percent": 75.098816
      }
    ],
    "last_active": 1790675964381,
    "protected": false,
    "status": "running",
    "clean_up": null
  },
  "stop": "Stopped qa-python :39107\n",
  "stop_exit": 0
}
POST /call refresh with valid token: 200 {"type":"result","id":7,"error":null}
token file permissions: 0o600

$ docker exec ppm-w3-ubuntu-arm64 cp /root/.local/bin/ppm /usr/local/bin/ppm
exit: 0
$ docker exec ppm-w3-ubuntu-arm64 su qa -s /bin/sh -c ppm list
PORT    NAME       BRANCH  MEMORY  CPU  UP  SESSION
:39106  qa-python           13 MB   0%  1m
exit: 0
$ docker exec ppm-w3-ubuntu-arm64 su qa -s /bin/sh -c ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39106,
      "pid": 10655,
      "root": {
        "pid": 10655,
        "started_at": 1790675976060
      },
      "cwd": "/work/python",
      "cwd_exists": true,
      "project": {
        "name": "qa-python",
        "root": "/work/python",
        "framework": "Python",
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 13123584,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": [
    {
      "port": 39102,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39190,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    }
  ]
}
exit: 0
$ docker exec ppm-w3-ubuntu-arm64 su qa -s /bin/sh -c ppm stop 39106 --force
Stopped qa-python :39106, killed
exit: 0
$ docker exec ppm-w3-ubuntu-arm64 sh -c curl -s --max-time 1 http://127.0.0.1:39106 >/dev/null; echo curl_exit:$?
curl_exit:7
exit: 0
```

</details>

<details>
<summary>Ubuntu: TUI navigation with separated key presses</summary>

```text
List; keys=[]; capture exit=0
                                              Servers

  56 MB                                                                          CPU (servers)  0%
  ██▅▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂
  ▅ Servers 56 MB   ▂ Other apps 625 MB   ▂ Free 1.2 of 1.9 GB
────────────────────────────────────────────────────────────────────────────────────────────────────

 ›:39102 qa linux                                                                ⠄⠄⠄⠄⠄⠄⠄⠄    43 MB
         qa node · up 1m

  :39190 /                                                                       ⠄⠄⠄⠄⠄⠄⠄⠄    14 MB
         / · up 9m

















────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Details   o Open   s Stop   r Restart   c Clean up   ? Keys   q Quit


Detail; keys=['Enter']; capture exit=0
  ‹                                           qa-node

  :39102                                                                           Running for <1m
────────────────────────────────────────────────────────────────────────────────────────────────────

  Branch       qa-linux
  Folder       /work/node
               3 more ▾   i

────────────────────────────────────────────────────────────────────────────────────────────────────

  Memory  43 MB                                                                             10 min
  2 GB │⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁
       │
       │
     0 │                                                                                         ⢀

  CPU  0%
  100% │
       │
       │
     0 │
  History fills in while ppm runs.

────────────────────────────────────────────────────────────────────────────────────────────────────

  Processes ▸   p                                                                        1 · 43 MB


────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Open localhost:39102   s Stop   r Restart   i More info   p Processes   ? Keys   esc Back


Help; keys=['?']; capture exit=0
                                                Keys

────────────────────────────────────────────────────────────────────────────────────────────────────

  Servers
  ↑ ↓  j k     Select
  ⏎            Details
  c            Clean up
  t            CPU for servers or the whole machine
  tab  m       Next machine

  Actions
  o            Open in browser
  v            Vercel preview
  r            Restart
  s            Stop

  Clean up
  space        Select or deselect
  a            Select all
  ⏎            Stop selected
  esc          Cancel

  Details
  ⏎            Open in browser
  i            More or less info
  p            Show or hide processes
  tab          Next server
  ↑ ↓          Scroll
────────────────────────────────────────────────────────────────────────────────────────────────────
  ↑ ↓ Scroll   esc Back


Back to detail; keys=['Escape']; capture exit=0
  ‹                                           qa-node

  :39102                                                                           Running for <1m
────────────────────────────────────────────────────────────────────────────────────────────────────

  Branch       qa-linux
  Folder       /work/node
               3 more ▾   i

────────────────────────────────────────────────────────────────────────────────────────────────────

  Memory  43 MB                                                                             10 min
  2 GB │⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁
       │
       │
     0 │                                                                                         ⢀

  CPU  0%
  100% │
       │
       │
     0 │
  History fills in while ppm runs.

────────────────────────────────────────────────────────────────────────────────────────────────────

  Processes ▸   p                                                                        1 · 43 MB


────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Open localhost:39102   s Stop   r Restart   i More info   p Processes   ? Keys   esc Back


Back to list; keys=['Escape']; capture exit=0
                                              Servers

  57 MB                                                                          CPU (servers)  0%
  ██▅▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂
  ▅ Servers 57 MB   ▂ Other apps 643 MB   ▂ Free 1.2 of 1.9 GB
────────────────────────────────────────────────────────────────────────────────────────────────────

 ›:39102 qa linux                                                                ⠄⠄⠄⠄⠄⠄⠄⠄    43 MB
         qa node · up 1m

  :39190 /                                                                       ⠄⠄⠄⠄⠄⠄⠄⠄    14 MB
         / · up 9m

















────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Details   o Open   s Stop   r Restart   c Clean up   ? Keys   q Quit


Quit; keys=['q']; capture exit=1
can't find pane: qa-final
```

</details>

<details>
<summary>Debian: installs, listing, actions, serve and doctor</summary>

```text
$ sh /src/scripts/install.sh
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190
Installed ppm 0.1.0 to /root/.local/bin/ppm
Add /root/.local/bin to your PATH, for example: export PATH="/root/.local/bin:$PATH"
exit: 0
$ /root/.local/bin/ppm --version
ppm 0.1.0
exit: 0
$ sha256sum /root/.local/bin/ppm /qa/artifacts/ppm-aarch64-unknown-linux-musl
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287  /root/.local/bin/ppm
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287  /qa/artifacts/ppm-aarch64-unknown-linux-musl
exit: 0
$ npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
exit: 0
$ uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Installed 1 package in 1ms
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
exit: 0
$ /root/.local/bin/ppm list
PORT    NAME               BRANCH    MEMORY  CPU   UP  SESSION
:39101  qa-python                     11 MB   0%   1m
:39102  qa-node            qa-linux   43 MB   0%   6m
:39103  /                             10 MB   0%  12m
:39104  deleted (deleted)             11 MB   0%   1m
:39190  /                             11 MB   0%   8m
exit: 0
$ /root/.local/bin/ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39101,
      "pid": 11758,
      "root": {
        "pid": 11758,
        "started_at": 1790675847480
      },
      "cwd": "/work/python",
      "cwd_exists": true,
      "project": {
        "name": "qa-python",
        "root": "/work/python",
        "framework": "Python",
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11200512,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39102,
      "pid": 11223,
      "root": {
        "pid": 11223,
        "started_at": 1790675482990
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 44723200,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39103,
      "pid": 11037,
      "root": {
        "pid": 11037,
        "started_at": 1790675127590
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 10396672,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39104,
      "pid": 11760,
      "root": {
        "pid": 11760,
        "started_at": 1790675847480
      },
      "cwd": "/work/deleted (deleted)",
      "cwd_exists": false,
      "project": {
        "name": "deleted (deleted)",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11233280,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "idle",
      "clean_up": {
        "kind": "worktree_deleted"
      }
    },
    {
      "port": 39190,
      "pid": 11057,
      "root": {
        "pid": 11057,
        "started_at": 1790675391070
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11725824,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
watch snapshots: 2
watch ports: [39101, 39102, 39103, 39104, 39190]
watch ports: [39101, 39102, 39103, 39104, 39190]
$ cp /root/.local/bin/ppm /usr/local/bin/ppm
exit: 0
$ su qa -s /bin/sh -c ppm list --json
[snapshot JSON projection]
{
  "servers": [],
  "other_ports": [
    {
      "port": 39101,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39102,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39103,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39104,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39190,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    }
  ]
}
exit: 0
$ /root/.local/bin/ppm doctor
ppm 0.1.0 · protocol 1 · Linux aarch64 · 2 cores
✓ Listening ports: 5 found
✓ Processes: 10 found
✓ Folder and command of your own processes
✓ Memory and CPU of your own processes
✓ Settings folder: /root/.config/port-process-manager
  Desktop app window blank on NVIDIA? Start it with WEBKIT_DISABLE_DMABUF_RENDERER=1.
exit: 0
$ /root/.local/bin/ppm serve --listen 0.0.0.0:39109
ppm: 0.0.0.0:39109 is not a loopback address; ppm serve listens only on loopback, so put a tunnel or proxy in front of it
exit: 1
GET /events missing token: 401
GET /events wrong token: 401
GET /events valid token: 200 text/event-stream
data: {"type":"hello","protocol":1,"ppm_version":"0.1.0","host":{"hostname":"19544cb0fbba","os":"linux","arch":"aarch64","cores":2}}

data: {"type":"snapshot", ...} [body omitted; listing checked separately]

$ sh -c cat /proc/net/tcp | awk '$2 ~ /:98C5$/ {print $2}'
0100007F:98C5
0100007F:98C5
0100007F:98C5
exit: 0
[Initial TUI transcript omitted; dedicated captures below.]
$ /root/.local/bin/ppm restart 39102
Restarted :39102 with node server.js
exit: 0
$ curl -fsS http://127.0.0.1:39102
qa-node
exit: 0
$ /root/.local/bin/ppm stop 39101
Stopped qa-python :39101
exit: 0
$ /root/.local/bin/ppm stop 39103 --force
Stopped / :39103, killed
exit: 0
$ /root/.local/bin/ppm clean --yes
:39104 deleted (deleted)           11 MB   Worktree deleted
Stopped 1 server, freeing 11 MB.
exit: 0
$ /root/.local/bin/ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39102,
      "pid": 11884,
      "root": {
        "pid": 11884,
        "started_at": 1790675892830
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 47441920,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39190,
      "pid": 11057,
      "root": {
        "pid": 11057,
        "started_at": 1790675391070
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 13486080,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
$ sh /src/scripts/install.sh
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190/bad
ppm install: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
$ npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
$ uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Installed 1 package in 1ms
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
```

</details>

<details>
<summary>Debian: normal-user installs, stdio, tree and HTTP call</summary>

```text
$ docker exec ppm-w3-debian-arm64 su qa -s /bin/sh -c export PPM_DOWNLOAD_URL=http://127.0.0.1:39190; sh /src/scripts/install.sh && npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version && XDG_CACHE_HOME=/home/qa/.cache/python-qa uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190
Installed ppm 0.1.0 to /home/qa/.local/bin/ppm
Add /home/qa/.local/bin to your PATH, for example: export PATH="/home/qa/.local/bin:$PATH"
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
Installed 1 package in 1ms
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0

$ docker exec -e QA_DISTRO=debian ppm-w3-debian-arm64 python3 /qa/extra.py
Nothing listening on ports 3000-65535.
$ su qa -s /bin/sh -c "ppm list"
$ ppm stdio (configure port 39102, 1-byte memory alert, 200ms interval; observe 33s)
handshake: {'type': 'hello', 'protocol': 1, 'ppm_version': '0.1.0', 'host': {'hostname': '19544cb0fbba', 'os': 'linux', 'arch': 'aarch64', 'cores': 2}}
configure results: [{'type': 'result', 'id': 1, 'error': None}]
alerts: [{'type': 'alert', 'port': 39102, 'kind': 'over_threshold', 'memory': 45777920}]
snapshots: 158
final server summary: [{'port': 39102, 'status': 'attention', 'memory': 45777920, 'cpu_percent': 0.0, 'history_samples': 4}]
stdio exit: 0

{
  "command": "ppm list --json (normal user, forked 32 MiB worker on 39107)",
  "server": {
    "port": 39107,
    "pid": 12119,
    "root": {
      "pid": 12119,
      "started_at": 1790675961680
    },
    "process_name": "python3",
    "addresses": [
      "0.0.0.0"
    ],
    "cwd": "/work/python",
    "cwd_exists": true,
    "command": "python3 tree.py",
    "launch_dir": "/work/python",
    "started_at": 1790675961680,
    "project": {
      "name": "qa-python",
      "root": "/work/python",
      "framework": "Python",
      "branch": null,
      "worktree": null,
      "github": null,
      "vercel": null
    },
    "workspace": null,
    "agent": null,
    "processes": [
      {
        "proc": {
          "pid": 12119,
          "started_at": 1790675961680
        },
        "name": "python3 tree.py",
        "depth": 0,
        "memory": 8581120,
        "cpu_percent": 0.0
      },
      {
        "proc": {
          "pid": 12125,
          "started_at": 1790675961740
        },
        "name": "python3 tree.py",
        "depth": 1,
        "memory": 39993344,
        "cpu_percent": 99.20635
      }
    ],
    "memory": 48574464,
    "cpu_percent": 99.20635,
    "connections": 0,
    "history": [
      {
        "at": 1790675965863,
        "memory": 48574464,
        "cpu_percent": 99.20635
      }
    ],
    "last_active": 1790675965863,
    "protected": false,
    "status": "running",
    "clean_up": null
  },
  "stop": "Stopped qa-python :39107\n",
  "stop_exit": 0
}
POST /call refresh with valid token: 200 {"type":"result","id":7,"error":null}
token file permissions: 0o600

$ docker exec ppm-w3-debian-arm64 cp /root/.local/bin/ppm /usr/local/bin/ppm
exit: 0
$ docker exec ppm-w3-debian-arm64 su qa -s /bin/sh -c ppm list
PORT    NAME       BRANCH  MEMORY  CPU  UP  SESSION
:39106  qa-python           12 MB   0%  1m
exit: 0
$ docker exec ppm-w3-debian-arm64 su qa -s /bin/sh -c ppm list --json
sh: 1: ppm: not found
exit: 127
$ docker exec ppm-w3-debian-arm64 su qa -s /bin/sh -c ppm stop 39106 --force
sh: 1: ppm: not found
exit: 127
$ docker exec ppm-w3-debian-arm64 sh -c curl -s --max-time 1 http://127.0.0.1:39106 >/dev/null; echo curl_exit:$?
curl_exit:0
exit: 0
```

</details>

<details>
<summary>Debian: TUI navigation with separated key presses</summary>

```text
List; keys=[]; capture exit=0
                                              Servers

  57 MB                                                                          CPU (servers)  0%
  ██▅▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂
  ▅ Servers 57 MB   ▂ Other apps 643 MB   ▂ Free 1.2 of 1.9 GB
────────────────────────────────────────────────────────────────────────────────────────────────────

 ›:39102 qa linux                                                                ⠄⠄⠄⠄⠄⠄⠄⠄    44 MB
         qa node · up 1m

  :39190 /                                                                       ⠄⠄⠄⠄⠄⠄⠄⠄    14 MB
         / · up 9m

















────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Details   o Open   s Stop   r Restart   c Clean up   ? Keys   q Quit


Detail; keys=['Enter']; capture exit=0
  ‹                                           qa-node

  :39102                                                                           Running for <1m
────────────────────────────────────────────────────────────────────────────────────────────────────

  Branch       qa-linux
  Folder       /work/node
               3 more ▾   i

────────────────────────────────────────────────────────────────────────────────────────────────────

  Memory  44 MB                                                                             10 min
  2 GB │⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁
       │
       │
     0 │                                                                                         ⢀

  CPU  0%
  100% │
       │
       │
     0 │
  History fills in while ppm runs.

────────────────────────────────────────────────────────────────────────────────────────────────────

  Processes ▸   p                                                                        1 · 44 MB


────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Open localhost:39102   s Stop   r Restart   i More info   p Processes   ? Keys   esc Back


Help; keys=['?']; capture exit=0
                                                Keys

────────────────────────────────────────────────────────────────────────────────────────────────────

  Servers
  ↑ ↓  j k     Select
  ⏎            Details
  c            Clean up
  t            CPU for servers or the whole machine
  tab  m       Next machine

  Actions
  o            Open in browser
  v            Vercel preview
  r            Restart
  s            Stop

  Clean up
  space        Select or deselect
  a            Select all
  ⏎            Stop selected
  esc          Cancel

  Details
  ⏎            Open in browser
  i            More or less info
  p            Show or hide processes
  tab          Next server
  ↑ ↓          Scroll
────────────────────────────────────────────────────────────────────────────────────────────────────
  ↑ ↓ Scroll   esc Back


Back to detail; keys=['Escape']; capture exit=0
  ‹                                           qa-node

  :39102                                                                           Running for <1m
────────────────────────────────────────────────────────────────────────────────────────────────────

  Branch       qa-linux
  Folder       /work/node
               3 more ▾   i

────────────────────────────────────────────────────────────────────────────────────────────────────

  Memory  44 MB                                                                             10 min
  2 GB │⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁
       │
       │
     0 │                                                                                         ⢀

  CPU  0%
  100% │
       │
       │
     0 │
  History fills in while ppm runs.

────────────────────────────────────────────────────────────────────────────────────────────────────

  Processes ▸   p                                                                        1 · 44 MB


────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Open localhost:39102   s Stop   r Restart   i More info   p Processes   ? Keys   esc Back


Back to list; keys=['Escape']; capture exit=0
                                              Servers

  57 MB                                                                        CPU (servers)  0.2%
  ██▅▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂
  ▅ Servers 57 MB   ▂ Other apps 636 MB   ▂ Free 1.2 of 1.9 GB
────────────────────────────────────────────────────────────────────────────────────────────────────

 ›:39102 qa linux                                                                ⠄⠄⠄⠄⠄⠄⠄⠄    44 MB
         qa node · up 1m

  :39190 /                                                                       ⠄⠄⠄⠄⠄⠄⠄⠄    14 MB
         / · up 9m

















────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Details   o Open   s Stop   r Restart   c Clean up   ? Keys   q Quit


Quit; keys=['q']; capture exit=1
can't find pane: qa-final
```

</details>

<details>
<summary>Alpine: installs, listing, actions, serve and doctor</summary>

```text
$ sh /src/scripts/install.sh
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190
Installed ppm 0.1.0 to /root/.local/bin/ppm
Add /root/.local/bin to your PATH, for example: export PATH="/root/.local/bin:$PATH"
exit: 0
$ /root/.local/bin/ppm --version
ppm 0.1.0
exit: 0
$ sha256sum /root/.local/bin/ppm /qa/artifacts/ppm-aarch64-unknown-linux-musl
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287  /root/.local/bin/ppm
4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287  /qa/artifacts/ppm-aarch64-unknown-linux-musl
exit: 0
$ npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
exit: 0
$ uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Installed 1 package in 1ms
realpath: --: No such file or directory
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
exit: 0
$ /root/.local/bin/ppm list
PORT    NAME               BRANCH    MEMORY  CPU   UP  SESSION
:39101  qa-python                     11 MB   0%   1m
:39102  qa-node            qa-linux   42 MB   0%   6m
:39103  /                             10 MB   0%  13m
:39104  deleted (deleted)             11 MB   0%   1m
:39190  /                             11 MB   0%   8m
exit: 0
$ /root/.local/bin/ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39101,
      "pid": 929,
      "root": {
        "pid": 929,
        "started_at": 1790675848610
      },
      "cwd": "/work/python",
      "cwd_exists": true,
      "project": {
        "name": "qa-python",
        "root": "/work/python",
        "framework": "Python",
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11244544,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39102,
      "pid": 423,
      "root": {
        "pid": 423,
        "started_at": 1790675490350
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 44479488,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39103,
      "pid": 33,
      "root": {
        "pid": 33,
        "started_at": 1790675069070
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 10285056,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39104,
      "pid": 931,
      "root": {
        "pid": 931,
        "started_at": 1790675848620
      },
      "cwd": "/work/deleted (deleted)",
      "cwd_exists": false,
      "project": {
        "name": "deleted (deleted)",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11336704,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "idle",
      "clean_up": {
        "kind": "worktree_deleted"
      }
    },
    {
      "port": 39190,
      "pid": 168,
      "root": {
        "pid": 168,
        "started_at": 1790675391130
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 11403264,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
watch snapshots: 2
watch ports: [39101, 39102, 39103, 39104, 39190]
watch ports: [39101, 39102, 39103, 39104, 39190]
$ cp /root/.local/bin/ppm /usr/local/bin/ppm
exit: 0
$ su qa -s /bin/sh -c ppm list --json
[snapshot JSON projection]
{
  "servers": [],
  "other_ports": [
    {
      "port": 39101,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39102,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39103,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39104,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    },
    {
      "port": 39190,
      "addresses": [
        "0.0.0.0"
      ],
      "owner": "root",
      "process_name": null
    }
  ]
}
exit: 0
$ /root/.local/bin/ppm doctor
ppm 0.1.0 · protocol 1 · Linux aarch64 · 2 cores
✓ Listening ports: 5 found
✓ Processes: 18 found
✓ Folder and command of your own processes
✓ Memory and CPU of your own processes
✓ Settings folder: /root/.config/port-process-manager
  Desktop app window blank on NVIDIA? Start it with WEBKIT_DISABLE_DMABUF_RENDERER=1.
exit: 0
$ /root/.local/bin/ppm serve --listen 0.0.0.0:39109
ppm: 0.0.0.0:39109 is not a loopback address; ppm serve listens only on loopback, so put a tunnel or proxy in front of it
exit: 1
GET /events missing token: 401
GET /events wrong token: 401
GET /events valid token: 200 text/event-stream
data: {"type":"hello","protocol":1,"ppm_version":"0.1.0","host":{"hostname":"905fe78a38d0","os":"linux","arch":"aarch64","cores":2}}

data: {"type":"snapshot", ...} [body omitted; listing checked separately]

$ sh -c cat /proc/net/tcp | awk '$2 ~ /:98C5$/ {print $2}'
0100007F:98C5
0100007F:98C5
0100007F:98C5
0100007F:98C5
exit: 0
[Initial TUI transcript omitted; dedicated captures below.]
$ /root/.local/bin/ppm restart 39102
Restarted :39102 with node server.js
exit: 0
$ curl -fsS http://127.0.0.1:39102
qa-node
exit: 0
$ /root/.local/bin/ppm stop 39101
Stopped qa-python :39101
exit: 0
$ /root/.local/bin/ppm stop 39103 --force
Stopped / :39103, killed
exit: 0
$ /root/.local/bin/ppm clean --yes
:39104 deleted (deleted)           11 MB   Worktree deleted
Stopped 1 server, freeing 11 MB.
exit: 0
$ /root/.local/bin/ppm list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39102,
      "pid": 1091,
      "root": {
        "pid": 1091,
        "started_at": 1790675900480
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 44369920,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39190,
      "pid": 168,
      "root": {
        "pid": 168,
        "started_at": 1790675391130
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 13109248,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
$ sh /src/scripts/install.sh
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190/bad
ppm install: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
$ npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
$ uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Installed 1 package in 1ms
realpath: --: No such file or directory
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm: checksum mismatch for ppm-aarch64-unknown-linux-musl: expected 4b181452e2f605e095ff0baed7eef8eae8169f5d2d67e994241bee0befadb287, got 06d0083ba740ff26c91cb17f10d15d398eea1affd3112599b429f30452b59db4
exit: 1
```

</details>

<details>
<summary>Alpine: normal-user installs, stdio, tree and HTTP call</summary>

```text
$ docker exec ppm-w3-alpine-arm64 su qa -s /bin/sh -c export PPM_DOWNLOAD_URL=http://127.0.0.1:39190; sh /src/scripts/install.sh && npx --yes --package=/qa/packages/port-process-manager-0.1.0.tgz ppm --version && XDG_CACHE_HOME=/home/qa/.cache/python-qa uvx --from /qa/packages/port_process_manager-0.1.0-py3-none-any.whl ppm --version
Downloading ppm-aarch64-unknown-linux-musl from http://127.0.0.1:39190
Installed ppm 0.1.0 to /home/qa/.local/bin/ppm
Add /home/qa/.local/bin to your PATH, for example: export PATH="/home/qa/.local/bin:$PATH"
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0
Installed 1 package in 1ms
realpath: --: No such file or directory
ppm: downloading ppm-aarch64-unknown-linux-musl 0.1.0
ppm 0.1.0

$ docker exec -e QA_DISTRO=alpine ppm-w3-alpine-arm64 python3 /qa/extra.py
Nothing listening on ports 3000-65535.
$ su qa -s /bin/sh -c "ppm list"
$ ppm stdio (configure port 39102, 1-byte memory alert, 200ms interval; observe 33s)
handshake: {'type': 'hello', 'protocol': 1, 'ppm_version': '0.1.0', 'host': {'hostname': '905fe78a38d0', 'os': 'linux', 'arch': 'aarch64', 'cores': 2}}
configure results: [{'type': 'result', 'id': 1, 'error': None}]
alerts: [{'type': 'alert', 'port': 39102, 'kind': 'over_threshold', 'memory': 45381632}]
snapshots: 158
final server summary: [{'port': 39102, 'status': 'attention', 'memory': 45381632, 'cpu_percent': 0.0, 'history_samples': 4}]
stdio exit: 0

{
  "command": "ppm list --json (normal user, forked 32 MiB worker on 39107)",
  "server": {
    "port": 39107,
    "pid": 1322,
    "root": {
      "pid": 1322,
      "started_at": 1790675961730
    },
    "process_name": "python3",
    "addresses": [
      "0.0.0.0"
    ],
    "cwd": "/work/python",
    "cwd_exists": true,
    "command": "python3 tree.py",
    "launch_dir": "/work/python",
    "started_at": 1790675961730,
    "project": {
      "name": "qa-python",
      "root": "/work/python",
      "framework": "Python",
      "branch": null,
      "worktree": null,
      "github": null,
      "vercel": null
    },
    "workspace": null,
    "agent": null,
    "processes": [
      {
        "proc": {
          "pid": 1322,
          "started_at": 1790675961730
        },
        "name": "python3 tree.py",
        "depth": 0,
        "memory": 8282112,
        "cpu_percent": 0.0
      },
      {
        "proc": {
          "pid": 1329,
          "started_at": 1790675961780
        },
        "name": "python3 tree.py",
        "depth": 1,
        "memory": 39731200,
        "cpu_percent": 98.81423
      }
    ],
    "memory": 48013312,
    "cpu_percent": 98.81423,
    "connections": 0,
    "history": [
      {
        "at": 1790675967212,
        "memory": 48013312,
        "cpu_percent": 98.81423
      }
    ],
    "last_active": 1790675967212,
    "protected": false,
    "status": "running",
    "clean_up": null
  },
  "stop": "Stopped qa-python :39107\n",
  "stop_exit": 0
}
POST /call refresh with valid token: 200 {"type":"result","id":7,"error":null}
token file permissions: 0o600

$ docker exec ppm-w3-alpine-arm64 cp /root/.local/bin/ppm /usr/local/bin/ppm
exit: 0
$ docker exec ppm-w3-alpine-arm64 su qa -s /bin/sh -c ppm list
sh: ppm: not found
exit: 127
$ docker exec ppm-w3-alpine-arm64 su qa -s /bin/sh -c ppm list --json
sh: ppm: not found
exit: 127
$ docker exec ppm-w3-alpine-arm64 su qa -s /bin/sh -c ppm stop 39106 --force
sh: ppm: not found
exit: 127
$ docker exec ppm-w3-alpine-arm64 sh -c curl -s --max-time 1 http://127.0.0.1:39106 >/dev/null; echo curl_exit:$?
curl_exit:0
exit: 0
```

</details>

<details>
<summary>Alpine: TUI navigation with separated key presses</summary>

```text
List; keys=[]; capture exit=0
                                              Servers

  57 MB                                                                          CPU (servers)  0%
  ██▅▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂
  ▅ Servers 57 MB   ▂ Other apps 642 MB   ▂ Free 1.2 of 1.9 GB
────────────────────────────────────────────────────────────────────────────────────────────────────

 ›:39102 qa linux                                                                ⠄⠄⠄⠄⠄⠄⠄⠄    43 MB
         qa node · up 1m

  :39190 /                                                                       ⠄⠄⠄⠄⠄⠄⠄⠄    13 MB
         / · up 9m

















────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Details   o Open   s Stop   r Restart   c Clean up   ? Keys   q Quit


Detail; keys=['Enter']; capture exit=0
  ‹                                           qa-node

  :39102                                                                           Running for <1m
────────────────────────────────────────────────────────────────────────────────────────────────────

  Branch       qa-linux
  Folder       /work/node
               3 more ▾   i

────────────────────────────────────────────────────────────────────────────────────────────────────

  Memory  43 MB                                                                             10 min
  2 GB │⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁
       │
       │
     0 │                                                                                         ⢀

  CPU  0%
  100% │
       │
       │
     0 │
  History fills in while ppm runs.

────────────────────────────────────────────────────────────────────────────────────────────────────

  Processes ▸   p                                                                        1 · 43 MB


────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Open localhost:39102   s Stop   r Restart   i More info   p Processes   ? Keys   esc Back


Help; keys=['?']; capture exit=0
                                                Keys

────────────────────────────────────────────────────────────────────────────────────────────────────

  Servers
  ↑ ↓  j k     Select
  ⏎            Details
  c            Clean up
  t            CPU for servers or the whole machine
  tab  m       Next machine

  Actions
  o            Open in browser
  v            Vercel preview
  r            Restart
  s            Stop

  Clean up
  space        Select or deselect
  a            Select all
  ⏎            Stop selected
  esc          Cancel

  Details
  ⏎            Open in browser
  i            More or less info
  p            Show or hide processes
  tab          Next server
  ↑ ↓          Scroll
────────────────────────────────────────────────────────────────────────────────────────────────────
  ↑ ↓ Scroll   esc Back


Back to detail; keys=['Escape']; capture exit=0
  ‹                                           qa-node

  :39102                                                                           Running for <1m
────────────────────────────────────────────────────────────────────────────────────────────────────

  Branch       qa-linux
  Folder       /work/node
               3 more ▾   i

────────────────────────────────────────────────────────────────────────────────────────────────────

  Memory  43 MB                                                                             10 min
  2 GB │⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁⠁
       │
       │
     0 │                                                                                         ⢀

  CPU  0%
  100% │
       │
       │
     0 │
  History fills in while ppm runs.

────────────────────────────────────────────────────────────────────────────────────────────────────

  Processes ▸   p                                                                        1 · 43 MB


────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Open localhost:39102   s Stop   r Restart   i More info   p Processes   ? Keys   esc Back


Back to list; keys=['Escape']; capture exit=0
                                              Servers

  57 MB                                                                        CPU (servers)  0.2%
  ██▅▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂
  ▅ Servers 57 MB   ▂ Other apps 644 MB   ▂ Free 1.2 of 1.9 GB
────────────────────────────────────────────────────────────────────────────────────────────────────

 ›:39102 qa linux                                                                ⠄⠄⠄⠄⠄⠄⠄⠄    43 MB
         qa node · up 1m

  :39190 /                                                                       ⠄⠄⠄⠄⠄⠄⠄⠄    13 MB
         / · up 9m

















────────────────────────────────────────────────────────────────────────────────────────────────────
  ⏎ Details   o Open   s Stop   r Restart   c Clean up   ? Keys   q Quit


Quit; keys=['q']; capture exit=1
can't find pane: qa-final
```

</details>

<details>
<summary>Mac to each Linux distro: remote install and forwarding</summary>

```text

DISTRO ubuntu
$ docker exec ppm-w3-ubuntu-arm64 sh -c mv /root/.local/bin/ppm /tmp/installed-ppm; rm /usr/local/bin/ppm
exit: 0
$ ppm remote add ubuntu -- docker exec -i ppm-w3-ubuntu-arm64
Added ubuntu. Run `ppm --on ubuntu` to see its servers.
exit: 0
$ ppm --on ubuntu list
ppm: ppm isn't installed on ubuntu. Run again with --yes to install it.
exit: 1
interactive install (answered y):
Install ppm on ubuntu? [y/N] y
Installing ppm 0.1.0 on ubuntu…
Installed ppm in /root/.local/bin/ppm on ubuntu.
PORT    NAME     BRANCH    MEMORY  CPU  UP  SESSION
:39102  qa-node  qa-linux   43 MB   0%  1m
:39190  /                   16 MB   0%  9m

$ ppm --on ubuntu list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39102,
      "pid": 10373,
      "root": {
        "pid": 10373,
        "started_at": 1790675892970
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 45289472,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39190,
      "pid": 9542,
      "root": {
        "pid": 9542,
        "started_at": 1790675391020
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 16995328,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
forward HTTP: 200 qa-node

Opened http://127.0.0.1:39102, forwarded from :39102 on ubuntu. Press Ctrl-C to stop forwarding.

$ ppm remote rm ubuntu
Removed ubuntu.
exit: 0

DISTRO debian
$ docker exec ppm-w3-debian-arm64 sh -c mv /root/.local/bin/ppm /tmp/installed-ppm; rm /usr/local/bin/ppm
exit: 0
$ ppm remote add debian -- docker exec -i ppm-w3-debian-arm64
Added debian. Run `ppm --on debian` to see its servers.
exit: 0
$ ppm --on debian list
ppm: ppm isn't installed on debian. Run again with --yes to install it.
exit: 1
interactive install (answered y):
Install ppm on debian? [y/N] y
Installing ppm 0.1.0 on debian…
Installed ppm in /root/.local/bin/ppm on debian.
PORT    NAME     BRANCH    MEMORY  CPU  UP  SESSION
:39102  qa-node  qa-linux   43 MB   0%  1m
:39190  /                   13 MB   0%  9m

$ ppm --on debian list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39102,
      "pid": 11884,
      "root": {
        "pid": 11884,
        "started_at": 1790675892830
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 45308928,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39190,
      "pid": 11057,
      "root": {
        "pid": 11057,
        "started_at": 1790675391070
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 13499392,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
forward HTTP: 200 qa-node

Opened http://127.0.0.1:39102, forwarded from :39102 on debian. Press Ctrl-C to stop forwarding.

$ ppm remote rm debian
Removed debian.
exit: 0

DISTRO alpine
$ docker exec ppm-w3-alpine-arm64 sh -c mv /root/.local/bin/ppm /tmp/installed-ppm; rm /usr/local/bin/ppm
exit: 0
$ ppm remote add alpine -- docker exec -i ppm-w3-alpine-arm64
Added alpine. Run `ppm --on alpine` to see its servers.
exit: 0
$ ppm --on alpine list
ppm: ppm isn't installed on alpine. Run again with --yes to install it.
exit: 1
interactive install (answered y):
Install ppm on alpine? [y/N] y
Installing ppm 0.1.0 on alpine…
Installed ppm in /root/.local/bin/ppm on alpine.
PORT    NAME     BRANCH    MEMORY  CPU  UP  SESSION
:39102  qa-node  qa-linux   43 MB   0%  1m
:39190  /                   13 MB   0%  9m

$ ppm --on alpine list --json
[snapshot JSON projection]
{
  "servers": [
    {
      "port": 39102,
      "pid": 1091,
      "root": {
        "pid": 1091,
        "started_at": 1790675900480
      },
      "cwd": "/work/node",
      "cwd_exists": true,
      "project": {
        "name": "qa-node",
        "root": "/work/node",
        "framework": "Express",
        "branch": "qa-linux",
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 45128704,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    },
    {
      "port": 39190,
      "pid": 168,
      "root": {
        "pid": 168,
        "started_at": 1790675391130
      },
      "cwd": "/",
      "cwd_exists": true,
      "project": {
        "name": "/",
        "root": null,
        "framework": null,
        "branch": null,
        "worktree": null,
        "github": null,
        "vercel": null
      },
      "memory": 13202432,
      "cpu_percent": 0.0,
      "protected": false,
      "status": "running",
      "clean_up": null
    }
  ],
  "other_ports": []
}
exit: 0
forward HTTP: 200 qa-node

Opened http://127.0.0.1:39102, forwarded from :39102 on alpine. Press Ctrl-C to stop forwarding.

$ ppm remote rm alpine
Removed alpine.
exit: 0
```

</details>

## Checks and remaining work

`pnpm install --frozen-lockfile` and `pnpm check` passed on the Mac (typecheck,
format, Clippy with warnings denied, workspace tests). No contract changes.
The Linux binary was built from source and exercised above; the desktop was not
built inside these minimal containers. CI status belongs to the PR checks.

Follow-ups: CLI rendering of `other_ports`, Linux
permission wording, x86_64 runtime coverage on a capable runner, protected-action
checks after #13, and the unverified integrations listed above.

All ten containers created for this run and the temporary build-cache image were
removed after evidence was copied out. Remote machine entries and forwarding
processes were removed/stopped. Shared Docker base images were left in place.
