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

`dactyl-db` 0.11.0 creates maintenance temporary files
with ambient modes. Decapod confines online-backup work in a fresh `0700`
sibling staging directory. After Dactyl verifies the snapshot, Decapod assigns
the new snapshot inode its private (or explicitly shared) final mode, syncs it,
and publishes without overwriting an existing destination. Staging is removed
on success or failure. Private transient Dactyl modes are therefore not
promised to be `0600`; the effective access boundary is the private directory.

The Dactyl 0.11.0 recovery API cannot select a staging directory or creation mode:
it requires the archive and active database to share a parent and creates the
rebuild temporary beside the active file. Decapod therefore permits recovery
only in a private `0700` directory on Unix. Successful replacement preserves
the archived original's safe mode on the **new** rebuilt inode. The original
archive's mode is untouched. If Dactyl fails partway through, retain its error
and files for supported operator recovery; do not assume success or retry by
editing SQLite files directly.

For a shared installation, an operator-controlled recovery plan is: quiesce
all writers, create an authorized consistent private copy through supported
backup tooling, repair the private copy, validate it, and explicitly approve
replacement of the shared store. Decapod does not perform an unsafe live file
copy or change shared-directory modes to force recovery through.

Shared-directory recovery remains unsupported until Dactyl exposes secure
maintenance creation modes (ideally preserving the source mode on rebuild)
or configurable private staging. This change does not claim to fix all
Dactyl consumers or to complete that upstream capability.

## Existing installations

`STORAGE_UNSAFE_PERMISSIONS` reports the offending path and stops without
repairing it. First stop writers and review ownership, intended sharing, ACLs
and links. A human owner can then remove unintended write access or relocate
state through an approved backup/recovery workflow. For an intentional trusted
group, use the bounded datastore opt-in above. Do not recursively chmod a
repository, delete lock files, or replace a database as an automatic repair.
`STORAGE_RECOVERY_PRIVATE_DIRECTORY_REQUIRED` describes the separate pinned
Dactyl recovery limitation. No automatic chmod is performed on existing files
or directories.

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
