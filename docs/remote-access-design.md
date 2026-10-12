# Private remote Web access: requirements and design

Status: discussion record and proposed design; not implemented or deployed.
Last reviewed: 2026-10-12 (Asia/Shanghai).
Related work: [Issue #52](https://github.com/singlelektron/ledger_rs/issues/52).

This document is for contributors designing private access to a personal ledger
from an iPhone. It records the owner's requirements, connectivity constraints,
and the engineering work needed before remote use. Update it when those
requirements change, connectivity is verified, or an implementation is accepted.
Track implementation tasks and their execution evidence in Issues and PRs.

## Confirmed requirements

- The Arch Linux computer holds the authoritative ledger and can remain on.
- The iPhone uses a browser to read and modify that ledger.
- When the computer is unreachable, asleep, or off, waiting until it returns is
  acceptable. A complete offline phone copy and offline editing are not required.
- The ledger must not be stored in a cloud service. Transmission must be
  authenticated and encrypted.
- The owner also rejects third-party relay services, including relays that can
  only forward ciphertext, and third-party device registration/connection
  metadata. This stricter requirement remains in effect: discussing Tailscale
  as an alternative did not authorize it for this feature.
- Access from campus Wi-Fi, cellular, and off-campus networks is desirable,
  subject to the connectivity limits below.
- Only the computer and iPhone are available. There is no additional home
  router, NAS, server, or other independently reachable device available as an
  entry point.
- The iPhone already uses a VPN/proxy app; temporarily switching it for ledger
  access is acceptable. The particular app and its routing behavior remain
  unverified.

The service restriction does not make ordinary network infrastructure invisible:
campus networks and carriers can still observe network addresses, connection
timing, and traffic volume. Encryption protects content, not all metadata.

## Relationship to Issue #52

Issue #52 describes a broader future system: complete local copies on multiple
devices, independent offline writes, and authenticated synchronization with
duplicate prevention, deletion tracking, and explicit conflict resolution.

The immediate request needs one authoritative database and remote browser
operations. It does not require peer replication, globally unique IDs for all
entities, tombstones, or an offline merge protocol. The phone's confirmed writes
are committed directly to the computer's database.

Keep full offline synchronization as separate future work. Neither this document
nor a remote-access implementation would fully resolve Issue #52. Do not sync
live SQLite database/WAL files with generic file-sync tools.

## Connectivity evidence and limits

Read-only inspection during the 2026-10-12 discussion found:

- Both physical network interfaces had campus-private IPv4 addresses.
- Neither physical interface had a public IPv6 address or IPv6 default route;
  IPv6 was not disabled in the inspected kernel settings.
- The wireless adapter advertised AP/hotspot support. This is capability
  evidence, not proof of a working hotspot or successful iPhone access.
- A Tailscale interface existed, but its presence did not establish whether a
  particular connection was direct or relayed.

The owner reported that campus devices appeared unable to reach each other and
that Mac-to-Arch SSH previously required Tailscale. Campus isolation was not
independently reproduced. There was no external ingress test, router/port-map
verification, or iPhone Safari acceptance test. These observations are a dated
snapshot, not a permanent claim about the campus network. Raw device identifiers,
addresses, and network configuration are intentionally omitted.

| Path | Assessment under the confirmed requirements |
| --- | --- |
| iPhone joins a private hotspot on the computer | Most controllable nearby path; adapter support observed, actual operation untested |
| Both devices use the campus network | Conditional on routing and isolation policy; a shared campus or SSID does not establish reachability |
| Cellular/off-campus access to the computer | No verified reachable entry point in the inspected configuration |
| Future public IPv6 or public IPv4/port mapping | Possible if the network provides it and permits ingress; requires actual external tests |
| An additional owner-controlled reachable gateway | Discussed alternative, unavailable with the current two-device setup |

If campus isolation prevents local access, there is no reachable public endpoint,
and third-party coordination/relaying is excluded, software cannot guarantee a
cross-network path. WireGuard encrypts reachable traffic; its keepalive feature
maintains an existing NAT mapping rather than creating public ingress. See the
[WireGuard quick start](https://www.wireguard.com/quickstart/).

Public IPv6 would still require an IPv6-capable path from the phone and suitable
firewall rules. An IPv4-only client cannot directly reach an IPv6-only endpoint.
Changing endpoint addresses also need an update strategy compatible with the
privacy requirement; do not silently introduce hosted dynamic DNS. Installing
Headscale on the same unreachable computer would not itself provide ingress;
its [deployment requirements](https://headscale.net/stable/setup/requirements/)
include a reachable server.

## Proposed architecture

```text
iPhone Safari
  -> HTTPS inside a directly connected WireGuard tunnel
  -> Caddy on the computer, listening only on the tunnel address
  -> ledger_web on loopback
  -> the computer's authoritative SQLite ledger
```

Start by validating a nearby path. If campus routing permits it, use that path;
otherwise evaluate a password-protected computer hotspot while retaining the
computer's wired campus connection. Hotspot-local ledger access need not depend
on an upstream Internet connection. AP advertisement alone does not validate
driver behavior, firewall rules, or phone connectivity. Do not reconfigure the
active desktop network merely to document this proposal.

Pair the two WireGuard peers through locally exchanged keys. Restrict tunnel
access to the ledger endpoint and support revoking the phone's key. Use a subnet
that does not conflict with existing networks, and avoid routing unrelated phone
traffic through the computer. Verify behavior when switching the existing VPN.

Use Caddy's local certificate authority for HTTPS, with no public certificate
issuance or hosted DNS dependency. Provision the CA certificate to the phone
through a trusted local exchange and protect the CA private key on the computer.
The phone must explicitly trust the certificate for Safari; do not instruct it
to bypass certificate errors. See [Caddy local HTTPS](https://caddyserver.com/docs/running)
and [Apple's certificate trust instructions](https://support.apple.com/en-gb/102390).
The fixed tunnel address can give Safari a stable URL even if the outer network
endpoint changes; discovering and updating that outer endpoint remains separate.

The proposed first version stores no complete offline browser ledger. Host
availability is required for reads and writes. Keep sensitive responses out of
persistent browser caches, avoid cloud analytics/external page assets, and review
export/download destinations before claiming that data stays off cloud services.

## Tailscale alternative: requires a changed privacy decision

Two distinct choices must not be conflated:

| Privacy boundary | Consequence |
| --- | --- |
| No cloud ledger or readable ledger content, but coordination metadata and encrypted relays are acceptable | Tailscale is a candidate, subject to real connectivity and security tests |
| No third-party coordination metadata or relay service at all | Tailscale's hosted service does not meet the requirement; this is the current choice |

Tailscale can establish direct device-to-device traffic or fall back to an
encrypted DERP relay. Requiring Tailscale for SSH does not prove that application
traffic always traverses a relay. Conversely, seeing a connection marked
`direct` does not establish that no third-party coordination or earlier relay
traffic occurred. See [connection types](https://tailscale.com/docs/reference/connection-types)
and [DERP encryption](https://tailscale.com/docs/reference/derp-servers).

If the owner explicitly relaxes the boundary, evaluate private Tailscale Serve
access, restricted to approved devices, with HTTPS terminated on the computer.
Do not expose the ledger through public Funnel. Hosted coordination still
involves device/connection information; automatic public HTTPS certificates also
publish certificate names in Certificate Transparency logs. See
[Serve](https://tailscale.com/docs/features/tailscale-serve),
[the privacy policy](https://tailscale.com/legal/privacy-policy), and
[HTTPS certificate metadata](https://tailscale.com/docs/how-to/set-up-https-certificates).
This alternative is neither selected nor a guarantee of campus connectivity.

## Required application work

Implementation facts below were rechecked at repository revision `bcd92a8`.
The current release remains local-only. Installing a tunnel or proxy alone does
not make its Web interface ready for remote use.

### Explicit remote security boundary

The executable rejects non-loopback listen addresses, and middleware rejects
non-loopback Host headers and expects `http://<Host>` for a supplied mutation
Origin. There is no application login/session layer. See the inspected
[listener](https://github.com/singlelektron/ledger_rs/blob/bcd92a8009c34e4d247e115070dada156b84fe5c/src/web/mod.rs)
and [middleware](https://github.com/singlelektron/ledger_rs/blob/bcd92a8009c34e4d247e115070dada156b84fe5c/src/web/middleware.rs).

Keep the default local-only mode. A future opt-in remote mode must define an
exact permitted HTTPS origin, trusted proxy boundary, and device authorization.
Bind the backend to loopback; keep the proxy reachable only by authorized tunnel
peers. Preserve Host, Origin, and cross-site request protection. Do not fix proxy
compatibility by disabling checks or indiscriminately rewriting Origin headers.
Document whether paired-device identity is sufficient or an application session
is also required; cookie sessions require secure cookies and CSRF protection.
Untrusted forwarded headers must not grant identity or bypass authorization.
Protect read/export/backup routes as well as writes.

### Durable duplicate prevention

Current transaction creation, batch entry, and CSV import can create additional
records if a successful request is submitted again. A lost response after commit
is therefore a financial correctness problem, not just a connection error.

Give each logical mutation a unique operation ID. Persist its authorization
scope, payload fingerprint, and result atomically with the business write. A
retry with the same ID and payload returns the original result, including after
a server restart; reuse with a different payload is rejected. Concurrent retries
must not create duplicate writes or audit entries. Cover all remotely enabled
mutations, including imports, or explicitly leave unsupported workflows local.
Define operation-record retention and expired-retry behavior before implementation.

### Detect stale edits

Current transaction updates target the record ID without checking the version
originally displayed to the user. A stale phone form can overwrite a newer edit.
See the inspected [SQLite implementation](https://github.com/singlelektron/ledger_rs/blob/bcd92a8009c34e4d247e115070dada156b84fe5c/src/infrastructure/sqlite.rs).

Introduce an atomic expected-version check for edits/deletes, returning a clear
conflict rather than silently overwriting. All writers, including CLI, TUI, Web,
and relevant import paths, must preserve the same version semantics. Existing
audit capture and database locking do not replace this check. Keep domain and
financial rules shared; do not implement competing rules in a phone frontend.

### Honest connection and save feedback

Distinguish confirmed saved, disconnected, and outcome unknown. Only a server
confirmation establishes success. After a timeout, query or safely replay the
same operation ID to establish its outcome; do not issue a fresh mutation blindly.
Preserve an entered form through recoverable failures where practical without
promising offline persistence. Offline drafts and full offline replication remain
separate scope decisions.

## Delivery gates and acceptance

1. **Connectivity:** use synthetic data to verify the private hotspot or permitted
   campus path with the actual iPhone. Treat off-campus access as blocked until a
   reachable ingress path is demonstrated under the approved privacy boundary.
2. **Security:** validate certificates in Safari, key pairing and revocation,
   unauthorized-device rejection, and Host/Origin/proxy protections. Confirm the
   backend and proxy are not inadvertently exposed on other interfaces. Review
   any application-session choice before implementing the remote mode.
3. **Reliability:** disconnect after commit but before the response, retry after
   a server restart, and submit simultaneous duplicates. Expect one logical write
   and audit effect. Exercise stale phone/desktop edits and edit/delete conflicts.
4. **Real-device use:** verify read/create/edit operations, VPN switching,
   reconnects, and explicit unavailable/unknown-result feedback in iPhone Safari.
   Test cellular and off-campus access only when an eligible route exists; report
   untested or blocked paths explicitly. Ping, a health endpoint, and desktop
   viewport emulation are insufficient evidence of phone usability.
5. **Recovery:** verify independent, consistent backups and restoration into a
   separate test database. A backup on the same disk does not protect against
   disk failure. External/offline backup media can be added when available;
   synchronization or remote access is not a backup strategy. Follow the
   [database guide](database.md#backup-and-recovery).

Implementation should follow focused branches and PRs, with security and
persistence changes receiving independent technical review. No deployment,
network reconfiguration, acceptance, or implementation is established by this
design record. The current recommendation is to validate private nearby access;
cross-network availability remains conditional on a reachable entry point or an
explicitly changed privacy requirement.
