
CREATE TABLE IF NOT EXISTS meta (
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY(namespace, key)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT DEFAULT '',
    tags TEXT DEFAULT '',
    owner TEXT DEFAULT '',
    due TEXT,
    ref TEXT DEFAULT '',
    status TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    closed_at TEXT,
    dir_path TEXT NOT NULL,
    scope TEXT NOT NULL,
    parent_task_id TEXT,
    priority TEXT DEFAULT 'medium',
    depends_on TEXT DEFAULT '',
    blocks TEXT DEFAULT '',
    category TEXT DEFAULT '',
    component TEXT DEFAULT '',
    assigned_to TEXT DEFAULT '',
    assigned_at TEXT,
    one_shot INTEGER DEFAULT 0,
    lease_expires_at TEXT,
    lease_generation INTEGER DEFAULT 0,
    lease_lifecycle TEXT DEFAULT '',
    intent_anchor TEXT DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS task_owners (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    claimed_at TEXT NOT NULL,
    claim_type TEXT NOT NULL DEFAULT 'primary',
    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_task_owners_task_agent ON task_owners(task_id, agent_id);

CREATE TABLE IF NOT EXISTS task_dependencies (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    depends_on_task_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(task_id, depends_on_task_id),
    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY(depends_on_task_id) REFERENCES tasks(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_task_dependencies_depends_on ON task_dependencies(depends_on_task_id);

CREATE TABLE IF NOT EXISTS task_tags (
    task_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY(task_id, tag),
    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_task_tags_tag ON task_tags(tag);

CREATE TABLE IF NOT EXISTS task_verification (
    todo_id TEXT PRIMARY KEY,
    proof_plan TEXT NOT NULL DEFAULT '[]',
    verification_artifacts TEXT,
    last_verified_at TEXT,
    last_verified_status TEXT,
    last_verified_notes TEXT,
    verification_policy_days INTEGER NOT NULL DEFAULT 90,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(todo_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS agent_prompts (
    id TEXT PRIMARY KEY,
    context TEXT NOT NULL,
    prompt_text TEXT NOT NULL,
    priority INTEGER DEFAULT 100,
    active INTEGER DEFAULT 1,
    usage_count INTEGER DEFAULT 0,
    last_shown_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS summaries (
    summary_hash TEXT PRIMARY KEY,
    ts TEXT NOT NULL,
    scope TEXT NOT NULL,
    original_hashes TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    token_estimate INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    event_id TEXT PRIMARY KEY,
    ts TEXT NOT NULL,
    seq INTEGER NOT NULL,
    stream TEXT NOT NULL,
    subject_kind TEXT,
    subject_id TEXT,
    event_type TEXT NOT NULL DEFAULT '',
    payload TEXT NOT NULL,
    actor TEXT NOT NULL DEFAULT 'decapod'
);


CREATE UNIQUE INDEX IF NOT EXISTS idx_events_stream_seq_unique ON events(stream, seq);
CREATE INDEX IF NOT EXISTS idx_events_stream_subject_seq ON events(stream, subject_kind, subject_id, seq) WHERE subject_kind IS NOT NULL AND subject_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS storage_components (
    component_kind TEXT NOT NULL,
    codec_version INTEGER NOT NULL,
    byte_len INTEGER NOT NULL CHECK(byte_len >= 0),
    content_sha256 TEXT NOT NULL,
    stored_bytes BLOB NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(component_kind, codec_version, byte_len, content_sha256)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS storage_component_refs (
    ref_scope TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    component_kind TEXT NOT NULL,
    codec_version INTEGER NOT NULL,
    byte_len INTEGER NOT NULL,
    content_sha256 TEXT NOT NULL,
    PRIMARY KEY(ref_scope, ref_id, component_kind, codec_version, byte_len, content_sha256),
    FOREIGN KEY(component_kind, codec_version, byte_len, content_sha256)
        REFERENCES storage_components(component_kind, codec_version, byte_len, content_sha256)
        ON DELETE RESTRICT
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_storage_component_refs_target
    ON storage_component_refs(component_kind, codec_version, byte_len, content_sha256);

CREATE TABLE IF NOT EXISTS storage_migration_manifests (
    migration_id TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    binary_version TEXT NOT NULL,
    pre_digest TEXT NOT NULL,
    post_digest TEXT NOT NULL,
    manifest_json TEXT NOT NULL
) WITHOUT ROWID;

INSERT INTO meta(namespace,key,value) VALUES ('todo','schema_version','18');
INSERT INTO meta(namespace,key,value) VALUES ('storage','format_version','2');
INSERT INTO meta(namespace,key,value) VALUES ('storage','min_reader_version','0.141.0');
INSERT INTO meta(namespace,key,value) VALUES ('storage','min_writer_version','0.141.0');
INSERT INTO tasks(id,hash,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at,one_shot,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision) VALUES ('feat_hist1937compat01','hist19','Historical compat task','Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody.','compat,hardening','hist-agent',NULL,'issue:1937','done','2026-01-10T12:00:00Z','2026-01-10T12:05:00Z','2026-01-10T12:05:00Z',NULL,'/work/compat-fixture','paths:["src/decapod/core/a1b2c3d4e5f6g7h8.rs","src/decapod/core/9i8j7k6l5m4n3o2p.rs","src/decapod/core/q1w2e3r4t5y6u7i8.rs","src/decapod/core/z9x8c7v6b5n4m3k2.rs"]',NULL,'high','','','core','storage','hist-agent','2026-01-10T12:00:00Z',1,'9999999999Z',1,'claimed','intent:compat:1937',2);
INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type) VALUES ('own_hist_1','feat_hist1937compat01','hist-agent','2026-01-10T12:00:00Z','primary');
INSERT INTO task_tags(task_id,tag) VALUES ('feat_hist1937compat01','compat');
INSERT INTO task_tags(task_id,tag) VALUES ('feat_hist1937compat01','hardening');
INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_at,last_verified_status,last_verified_notes,verification_policy_days,updated_at) VALUES ('feat_hist1937compat01','["validate_passes"]','{"task_id":"feat_hist1937compat01","proof_plan_results":[{"proof_gate":"validate_passes","output_hash":"sha256:historical1937"}]}','2026-01-10T12:05:00Z','passed','Original historical verification notes stay immutable.',90,'2026-01-10T12:05:00Z');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_ADD_1','2026-01-10T12:00:00Z',1,'todo','task','feat_hist1937compat01','task.add','{"_ce_v1":95,"payload":{"title":"Historical compat task"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('task-journal:85b272aa4a895c5979b54397cb20f3492c45c99cb1413299aa3cc9911b682729','2026-01-10T12:00:00Z',2,'todo','task','feat_hist1937compat01','task.journal.checkpoint','{"_ce_v1":95,"payload":{"schema_version":2,"task_id":"feat_hist1937compat01","ordinal":1,"kind":"observed_baseline","predecessor":null,"previous_digest":null,"source_watermark":1,"operation":"explicit_current_state_baseline","actor":"hist-agent","ts":"2026-01-10T12:00:00Z","projection":{"task":{"id":"feat_hist1937compat01","hash":"hist19","title":"Historical compat task","description":"","tags":"compat,hardening","owner":"hist-agent","due":null,"ref":"issue:1937","status":"in_progress","created_at":"2026-01-10T12:00:00Z","updated_at":"2026-01-10T12:00:00Z","completed_at":null,"closed_at":null,"dir_path":"/work/compat-fixture","scope":"","parent_task_id":null,"priority":"high","depends_on":"","blocks":"","category":"core","component":"storage","assigned_to":"hist-agent","assigned_at":"2026-01-10T12:00:00Z","one_shot":1,"lease_expires_at":"9999999999Z","lease_generation":1,"lease_lifecycle":"claimed","intent_anchor":"intent:compat:1937","revision":1},"owners":[{"id":"own_hist_1","agent_id":"hist-agent","claimed_at":"2026-01-10T12:00:00Z","claim_type":"primary"}],"dependencies":[],"tags":["compat","hardening"],"verification":{"proof_plan":"[\"validate_passes\"]","verification_artifacts":"","last_verified_at":"2026-01-10T12:00:00Z","last_verified_status":"passed","last_verified_notes":"Original historical verification notes stay immutable.","verification_policy_days":90,"updated_at":"2026-01-10T12:00:00Z"}},"component_refs":{"task.description":{"kind":"task.description","codec_version":1,"byte_len":1391,"content_sha256":"27bfbef375d365e96d8b26ad9fbe9cf533c9ba46c39400ea87978ff4a51c1e9f"},"task.scope":{"kind":"task.scope","codec_version":0,"byte_len":163,"content_sha256":"8a70e2bf9b2424862262ad955a33a5c7c03646eb6d897b0877b9d67e4a861359"},"verification.verification_artifacts":{"kind":"verification.verification_artifacts","codec_version":0,"byte_len":129,"content_sha256":"87b62d621c1e578396175c39373d39a533a89f7252f10e465b93d3b0c30f6b6f"}},"projection_digest":"96ade40a5120cf95805eaedd9797b50502ddf16789b69a28381559c883e54e58","record_digest":"85b272aa4a895c5979b54397cb20f3492c45c99cb1413299aa3cc9911b682729"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_DONE_1','2026-01-10T12:05:00Z',3,'todo','task','feat_hist1937compat01','task.done','{"_ce_v1":95,"payload":{"revision":2}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('task-journal:ffe25bd13d9eb9ddecbdc1f02c6f09382837f209296974d345a31d8c98a49038','2026-01-10T12:05:00Z',4,'todo','task','feat_hist1937compat01','task.journal.checkpoint','{"_ce_v1":95,"payload":{"schema_version":2,"task_id":"feat_hist1937compat01","ordinal":2,"kind":"transition","predecessor":"85b272aa4a895c5979b54397cb20f3492c45c99cb1413299aa3cc9911b682729","previous_digest":"96ade40a5120cf95805eaedd9797b50502ddf16789b69a28381559c883e54e58","source_watermark":3,"operation":"task.done","actor":"hist-agent","ts":"2026-01-10T12:05:00Z","projection":{"task":{"id":"feat_hist1937compat01","hash":"hist19","title":"Historical compat task","description":"","tags":"compat,hardening","owner":"hist-agent","due":null,"ref":"issue:1937","status":"done","created_at":"2026-01-10T12:00:00Z","updated_at":"2026-01-10T12:05:00Z","completed_at":"2026-01-10T12:05:00Z","closed_at":null,"dir_path":"/work/compat-fixture","scope":"","parent_task_id":null,"priority":"high","depends_on":"","blocks":"","category":"core","component":"storage","assigned_to":"hist-agent","assigned_at":"2026-01-10T12:00:00Z","one_shot":1,"lease_expires_at":"9999999999Z","lease_generation":1,"lease_lifecycle":"claimed","intent_anchor":"intent:compat:1937","revision":2},"owners":[{"id":"own_hist_1","agent_id":"hist-agent","claimed_at":"2026-01-10T12:00:00Z","claim_type":"primary"}],"dependencies":[],"tags":["compat","hardening"],"verification":{"proof_plan":"[\"validate_passes\"]","verification_artifacts":"","last_verified_at":"2026-01-10T12:05:00Z","last_verified_status":"passed","last_verified_notes":"Original historical verification notes stay immutable.","verification_policy_days":90,"updated_at":"2026-01-10T12:05:00Z"}},"component_refs":{"task.description":{"kind":"task.description","codec_version":1,"byte_len":1559,"content_sha256":"9bca2890c4ec267b6ccca297faba19bfb0de77af3daf2c12cd07fc179d6ee6f6"},"task.scope":{"kind":"task.scope","codec_version":0,"byte_len":163,"content_sha256":"8a70e2bf9b2424862262ad955a33a5c7c03646eb6d897b0877b9d67e4a861359"},"verification.verification_artifacts":{"kind":"verification.verification_artifacts","codec_version":0,"byte_len":129,"content_sha256":"87b62d621c1e578396175c39373d39a533a89f7252f10e465b93d3b0c30f6b6f"}},"projection_digest":"f751949ae363847f3ff9b1bd2eff8056e1e814e6944424c25b6555479e2f23a9","record_digest":"ffe25bd13d9eb9ddecbdc1f02c6f09382837f209296974d345a31d8c98a49038"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_COMMENT_1','2026-01-10T12:10:00Z',5,'todo','task','feat_hist1937compat01','task.comment','{"_ce_v1":95,"payload":{"comment":"Historical pre-codec comment"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_BROKER_1','2026-01-10T12:00:00Z',1,'broker',NULL,NULL,'broker.audit','{"_ce_v1":131023,"op":"todo.add"}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_FED_1','2026-01-10T12:00:00Z',1,'federation','node','node_hist_1','node.create','{"node_id":"node_hist_1","title":"Historical federation node","body":"Pre-codec federation context"}','hist-agent');
INSERT INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at) VALUES ('task.description',1,1391,'27bfbef375d365e96d8b26ad9fbe9cf533c9ba46c39400ea87978ff4a51c1e9f',X'4443504c434d31006f05000027bfbef375d365e96d8b26ad9fbe9cf533c9ba46c39400ea87978ff4a51c1e9f789cedcbe10980400806d055be0982b6b835c493902245ede6af111ac0ffef0dcdb250a60b6b47519e98921ceaa576c3435262c9047158261e3f823eb161f4ecd9b367cffff30505370bdf','2026-01-10T12:00:00Z');
INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256) VALUES ('task_checkpoint','task-journal:85b272aa4a895c5979b54397cb20f3492c45c99cb1413299aa3cc9911b682729','task.description',1,1391,'27bfbef375d365e96d8b26ad9fbe9cf533c9ba46c39400ea87978ff4a51c1e9f');
INSERT INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at) VALUES ('task.scope',0,163,'8a70e2bf9b2424862262ad955a33a5c7c03646eb6d897b0877b9d67e4a861359',X'70617468733a5b227372632f64656361706f642f636f72652f613162326333643465356636673768382e7273222c227372632f64656361706f642f636f72652f3969386a376b366c356d346e336f32702e7273222c227372632f64656361706f642f636f72652f713177326533723474357936753769382e7273222c227372632f64656361706f642f636f72652f7a3978386337763662356e346d336b322e7273225d','2026-01-10T12:00:00Z');
INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256) VALUES ('task_checkpoint','task-journal:85b272aa4a895c5979b54397cb20f3492c45c99cb1413299aa3cc9911b682729','task.scope',0,163,'8a70e2bf9b2424862262ad955a33a5c7c03646eb6d897b0877b9d67e4a861359');
INSERT INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at) VALUES ('verification.verification_artifacts',0,129,'87b62d621c1e578396175c39373d39a533a89f7252f10e465b93d3b0c30f6b6f',X'7b227461736b5f6964223a22666561745f6869737431393337636f6d7061743031222c2270726f6f665f706c616e5f726573756c7473223a5b7b2270726f6f665f67617465223a2276616c69646174655f706173736573222c226f75747075745f68617368223a227368613235363a686973746f726963616c31393337227d5d7d','2026-01-10T12:00:00Z');
INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256) VALUES ('task_checkpoint','task-journal:85b272aa4a895c5979b54397cb20f3492c45c99cb1413299aa3cc9911b682729','verification.verification_artifacts',0,129,'87b62d621c1e578396175c39373d39a533a89f7252f10e465b93d3b0c30f6b6f');
INSERT INTO storage_components(component_kind,codec_version,byte_len,content_sha256,stored_bytes,created_at) VALUES ('task.description',1,1559,'9bca2890c4ec267b6ccca297faba19bfb0de77af3daf2c12cd07fc179d6ee6f6',X'4443504c434d3100170600009bca2890c4ec267b6ccca297faba19bfb0de77af3daf2c12cd07fc179d6ee6f6789cedccd10980300c05c0553281a2bb748092540c62539a67c5ed750ec9ef7d5c6a925184c6328f9590fd2029ce5d1bd42add8afd03947e6a558732b56eb6115f0e9367a2144104114410c18f8217a9364407','2026-01-10T12:05:00Z');
INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256) VALUES ('task_checkpoint','task-journal:ffe25bd13d9eb9ddecbdc1f02c6f09382837f209296974d345a31d8c98a49038','task.description',1,1559,'9bca2890c4ec267b6ccca297faba19bfb0de77af3daf2c12cd07fc179d6ee6f6');
INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256) VALUES ('task_checkpoint','task-journal:ffe25bd13d9eb9ddecbdc1f02c6f09382837f209296974d345a31d8c98a49038','task.scope',0,163,'8a70e2bf9b2424862262ad955a33a5c7c03646eb6d897b0877b9d67e4a861359');
INSERT INTO storage_component_refs(ref_scope,ref_id,component_kind,codec_version,byte_len,content_sha256) VALUES ('task_checkpoint','task-journal:ffe25bd13d9eb9ddecbdc1f02c6f09382837f209296974d345a31d8c98a49038','verification.verification_artifacts',0,129,'87b62d621c1e578396175c39373d39a533a89f7252f10e465b93d3b0c30f6b6f');
