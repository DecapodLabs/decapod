# Local storage permissions

This is a hardening contract, not a report of an exploited vulnerability.
Decapod owns filesystem policy; Dactyl owns SQLite execution. The filesystem
must enforce its ordinary permissions and SQLite locking semantics. No blanket
FUSE/virtiofs rejection is introduced.

## Unix defaults

New database and coordination files, governance atomic-write temporaries,
runtime configuration and credential files are created with a maximum mode of
`0600`. Newly created state directories use `0700`. The process umask can remove
permissions; Decapod never changes the process-global umask. This applies at
creation, before writing bytes, rather than relying on a later chmod.

The boundary checks existing files and SQLite `-wal`, `-shm`, `-journal` and
`.lock` sidecars before opening a datastore. SQLite creates its ordinary
sidecars from the database's mode. State-file symlinks, hard-linked files,
non-regular files, unsafe directory ancestry and unintended group/world write
access are rejected. Unix file opens use `O_NOFOLLOW`. Both lexical and resolved
ancestry are checked, including symlink ownership. Root-owned system ancestry
is trusted; sticky ancestors such as `/tmp` are permitted for an owned private
child, not as the directory directly containing the datastore.

These checks assume the same account, root and any explicitly trusted sharing
group are trusted. They are not a sandbox against a malicious process with the
same authority. POSIX mode checks do not audit extended ACLs, mount policy or
other filesystem access mechanisms; the owner must review those separately.

## Explicit trusted-group datastore sharing

Normal host/container access by the same Unix account continues to use the
same datastore and coordination lock. Read-only logical connections can still
share the process-local lock registry. There is no filesystem-type ban added
for this hardening.

An installation deliberately sharing a datastore between Unix accounts can
set `DECAPOD_STORAGE_SHARED_GROUP=1` in each participating process. This trusts
the datastore directory's group, permits existing group-write modes, and
requests `0770` directories and `0660` database/lock files for new storage. The
umask still applies: use an appropriate group-preserving umask such as `007`.
The operator must arrange the intended group ownership, group-traversable
ancestry and setgid directory inheritance outside Decapod. Only the datastore
directory itself requests shared mode; newly needed ancestors remain private
until the owner explicitly configures them. Managed project/configuration
directories are private even when the datastore opt-in is set. Writable ancestors and files must use that same
group; world-write permission is never accepted. Existing modes and group
ownership are never silently changed.

The opt-in applies only to datastore paths and migration copies, not machine
credentials, sessions or runtime configuration. Keep machine-local state
private. It does not authorize unrelated members of other groups, nor make
shared credentials safe.

## Atomic replacement, migration and maintenance

Atomic governance/configuration writes create a new exclusive private file,
write and sync it, then rename. Failure removes only a temporary file created
by this operation. Existing unsafe targets are rejected before mutation.
Migration backup/restore copies use explicit creation modes rather than
`fs::copy`, which copies the source's mode. Migration ledgers use the same
atomic writer. The original source is not chmodded.

Online backup remains confined in a fresh `0700` sibling staging directory.
After Dactyl verifies the snapshot, Decapod assigns the new snapshot inode its
private (or explicitly shared) final mode, syncs it, and publishes without
overwriting an existing destination. Staging is removed on success or failure.

Decapod consumes the published `dactyl-db` 0.11.1 secure recovery primitive.
On Linux and Android, explicit `decapod data db recover --preserve-original-at
<unused-path>` supports a `0660` database in a trusted-group `2770` directory
with `DECAPOD_STORAGE_SHARED_GROUP=1`. The archive must be unused and share the
active database's parent. Dactyl creates same-filesystem private staging with
permissions bounded by `0700`, and rebuild/rollback-journal files bounded by
`0600` from creation even under umask `000`. More restrictive umasks can cause
refusal. Dactyl restores the original owner, group, and exact safe mode on the
rebuilt inode before activation. The archived original keeps its inode and
metadata; Decapod performs no post-activation chmod.

The primitive verifies the logical rebuild, archives the original and its
surviving sidecars, and activates using atomic no-replace rename. It never
clobbers an existing archive or a competing destination. Final journal mode is
DELETE. Normal trusted-group access remains available afterward. Existing
storage/ancestry checks still apply; world-writable storage is never accepted.

Quiesce all SQLite connections and filesystem name, content, and metadata
writers from connection open through completion. The same-user and trusted
group boundary is not a sandbox against a malicious directory writer. The
filesystem must support reliable sync and atomic no-replace rename. Extended
source ACLs/MAC labels or inherited directory policy that Dactyl cannot preserve
fail closed with `recovery_metadata_unsupported`. Failure to preserve ownership
returns `recovery_ownership_unavailable`. Other platforms return
`secure_recovery_unsupported`; ordinary read/write/backup remain independent.
No existing permissions or security policy are changed to force recovery.

Pre-activation failures preserve the original in place or try to restore its
archive. Post-activation failures attempt restoration and confine retained
`failed.db` diagnostics beneath private staging. If rollback is blocked, the
original may remain at its archive path and `recovery_rollback_failed` identifies
the failed restoration. Inspect the original typed error and preserved paths;
do not guess success or retry by editing SQLite files. Crashes are not automatic
repair triggers, and power-loss guarantees remain bounded by filesystem sync.
See [Dactyl's recovery contract](https://github.com/DecapodLabs/dactyl/blob/v0.11.1/docs/sqlite-recovery.md)
for upstream creation, failure-injection, and interruption proof.

## Existing installations

`STORAGE_UNSAFE_PERMISSIONS` reports the offending path and stops without
repairing it. First stop writers and review ownership, intended sharing, ACLs
and links. A human owner can then remove unintended write access or relocate
state through an approved backup/recovery workflow. For an intentional trusted
group, use the bounded datastore opt-in above. Do not recursively chmod a
repository, delete lock files, or replace a database as an automatic repair.
Typed Dactyl capability and conflict codes retain their identity in maintenance
diagnostics. No automatic chmod is performed on existing files or directories.

## Windows

Windows does not implement POSIX modes or `O_NOFOLLOW`. Creation uses the
account/directory's inherited ACLs; this change does not install or verify a
Windows DACL. Use a private user-profile directory and review inherited ACLs
with Windows tooling. Regular-file/symlink checks, exclusive temporary
creation and atomic publication still apply, but Unix mode/group tests are
Unix-only. Native Windows ACL security requires separate Windows-specific
implementation and validation; Unix permission bits must not be presented as
Windows protection.

## Proof

Unit tests launch a child shell with `umask 000`, then exec one exact Rust
test with one test thread. The parent test harness never changes its umask.
Tests cover datastore/lock/sidecar creation, atomic replacement, migration
backup/restore/ledgers, runtime config, session replacement, explicit shared
storage, rejected unsafe existing modes, symlinks and hard links. Existing
SQLite maintenance, session and migration regression suites remain required.

The group broker keeps its Unix socket beneath `data/broker-runtime`, created
as 0700 (private) or 0770 (explicit trusted group). Before binding it rejects an
existing runtime directory whose access bits exceed that boundary; it does not
chmod the directory. This prevents an ambient permissive umask from exposing
a transient world-connectable socket beneath a legacy write-safe 0755 data
directory. Election and marker files use the datastore sharing policy, while
credentials continue to use the private policy.
