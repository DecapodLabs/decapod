
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

CREATE INDEX IF NOT EXISTS idx_events_stream_seq ON events(stream, seq);
CREATE INDEX IF NOT EXISTS idx_events_subject ON events(subject_kind, subject_id);
INSERT INTO meta(namespace,key,value) VALUES ('todo','schema_version','18');
INSERT INTO tasks(id,hash,title,description,tags,owner,due,ref,status,created_at,updated_at,completed_at,closed_at,dir_path,scope,parent_task_id,priority,depends_on,blocks,category,component,assigned_to,assigned_at,one_shot,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision) VALUES ('feat_hist1937compat01','hist19','Historical compat task','Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody.','compat,hardening','hist-agent',NULL,'issue:1937','done','2026-01-10T12:00:00Z','2026-01-10T12:05:00Z','2026-01-10T12:05:00Z',NULL,'/work/compat-fixture','paths:["src/decapod/core/a1b2c3d4e5f6g7h8.rs","src/decapod/core/9i8j7k6l5m4n3o2p.rs","src/decapod/core/q1w2e3r4t5y6u7i8.rs","src/decapod/core/z9x8c7v6b5n4m3k2.rs"]',NULL,'high','','','core','storage','hist-agent','2026-01-10T12:00:00Z',1,'9999999999Z',1,'claimed','intent:compat:1937',2);
INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type) VALUES ('own_hist_1','feat_hist1937compat01','hist-agent','2026-01-10T12:00:00Z','primary');
INSERT INTO task_tags(task_id,tag) VALUES ('feat_hist1937compat01','compat');
INSERT INTO task_tags(task_id,tag) VALUES ('feat_hist1937compat01','hardening');
INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_at,last_verified_status,last_verified_notes,verification_policy_days,updated_at) VALUES ('feat_hist1937compat01','["validate_passes"]','{"task_id":"feat_hist1937compat01","proof_plan_results":[{"proof_gate":"validate_passes","output_hash":"sha256:historical1937"}]}','2026-01-10T12:05:00Z','passed','Original historical verification notes stay immutable.',90,'2026-01-10T12:05:00Z');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_ADD_1','2026-01-10T12:00:00Z',1,'todo','task','feat_hist1937compat01','task.add','{"event_id":"EV_V1_ADD_1","ts":"2026-01-10T12:00:00Z","event_type":"task.add","actor":"hist-agent","task_id":"feat_hist1937compat01","status":"success","payload":{"title":"Historical compat task"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('task-journal:99e04734e617fa45247e2eee15b3d3c9819a396070efdbddfba1b71d333f8a74','2026-01-10T12:00:00Z',2,'todo','task','feat_hist1937compat01','task.journal.checkpoint','{"ts":"2026-01-10T12:00:00Z","event_id":"task-journal:99e04734e617fa45247e2eee15b3d3c9819a396070efdbddfba1b71d333f8a74","event_type":"task.journal.checkpoint","status":"success","task_id":"feat_hist1937compat01","actor":"hist-agent","payload":{"schema_version":1,"task_id":"feat_hist1937compat01","ordinal":1,"kind":"observed_baseline","predecessor":null,"previous_digest":null,"source_watermark":1,"operation":"explicit_current_state_baseline","actor":"hist-agent","projection":{"task":{"id":"feat_hist1937compat01","hash":"hist19","title":"Historical compat task","description":"Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades. Historical v1 task description preserved across upgrades.","tags":"compat,hardening","owner":"hist-agent","due":null,"ref":"issue:1937","status":"in_progress","created_at":"2026-01-10T12:00:00Z","updated_at":"2026-01-10T12:00:00Z","completed_at":null,"closed_at":null,"dir_path":"/work/compat-fixture","scope":"paths:[\"src/decapod/core/a1b2c3d4e5f6g7h8.rs\",\"src/decapod/core/9i8j7k6l5m4n3o2p.rs\",\"src/decapod/core/q1w2e3r4t5y6u7i8.rs\",\"src/decapod/core/z9x8c7v6b5n4m3k2.rs\"]","parent_task_id":null,"priority":"high","depends_on":"","blocks":"","category":"core","component":"storage","assigned_to":"hist-agent","assigned_at":"2026-01-10T12:00:00Z","one_shot":1,"lease_expires_at":"9999999999Z","lease_generation":1,"lease_lifecycle":"claimed","intent_anchor":"intent:compat:1937","revision":1},"owners":[{"id":"own_hist_1","agent_id":"hist-agent","claimed_at":"2026-01-10T12:00:00Z","claim_type":"primary"}],"dependencies":[],"tags":["compat","hardening"],"verification":{"proof_plan":"[\"validate_passes\"]","verification_artifacts":"{\"task_id\":\"feat_hist1937compat01\",\"proof_plan_results\":[{\"proof_gate\":\"validate_passes\",\"output_hash\":\"sha256:historical1937\"}]}","last_verified_at":"2026-01-10T12:00:00Z","last_verified_status":"passed","last_verified_notes":"Original historical verification notes stay immutable.","verification_policy_days":90,"updated_at":"2026-01-10T12:00:00Z"}},"projection_digest":"96ade40a5120cf95805eaedd9797b50502ddf16789b69a28381559c883e54e58","record_digest":"99e04734e617fa45247e2eee15b3d3c9819a396070efdbddfba1b71d333f8a74"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_DONE_1','2026-01-10T12:05:00Z',3,'todo','task','feat_hist1937compat01','task.done','{"event_id":"EV_V1_DONE_1","ts":"2026-01-10T12:05:00Z","event_type":"task.done","actor":"hist-agent","task_id":"feat_hist1937compat01","status":"success","payload":{"revision":2}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('task-journal:a6d21a7d5d8b5c77c091fc31a9f409f37eb7bdc9d8079288d26e6e26e09d174c','2026-01-10T12:05:00Z',4,'todo','task','feat_hist1937compat01','task.journal.checkpoint','{"ts":"2026-01-10T12:05:00Z","event_id":"task-journal:a6d21a7d5d8b5c77c091fc31a9f409f37eb7bdc9d8079288d26e6e26e09d174c","event_type":"task.journal.checkpoint","status":"success","task_id":"feat_hist1937compat01","actor":"hist-agent","payload":{"schema_version":1,"task_id":"feat_hist1937compat01","ordinal":2,"kind":"transition","predecessor":"99e04734e617fa45247e2eee15b3d3c9819a396070efdbddfba1b71d333f8a74","previous_digest":"96ade40a5120cf95805eaedd9797b50502ddf16789b69a28381559c883e54e58","source_watermark":3,"operation":"task.done","actor":"hist-agent","projection":{"task":{"id":"feat_hist1937compat01","hash":"hist19","title":"Historical compat task","description":"Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody. Updated v1/v2 task description with deterministic proof custody.","tags":"compat,hardening","owner":"hist-agent","due":null,"ref":"issue:1937","status":"done","created_at":"2026-01-10T12:00:00Z","updated_at":"2026-01-10T12:05:00Z","completed_at":"2026-01-10T12:05:00Z","closed_at":null,"dir_path":"/work/compat-fixture","scope":"paths:[\"src/decapod/core/a1b2c3d4e5f6g7h8.rs\",\"src/decapod/core/9i8j7k6l5m4n3o2p.rs\",\"src/decapod/core/q1w2e3r4t5y6u7i8.rs\",\"src/decapod/core/z9x8c7v6b5n4m3k2.rs\"]","parent_task_id":null,"priority":"high","depends_on":"","blocks":"","category":"core","component":"storage","assigned_to":"hist-agent","assigned_at":"2026-01-10T12:00:00Z","one_shot":1,"lease_expires_at":"9999999999Z","lease_generation":1,"lease_lifecycle":"claimed","intent_anchor":"intent:compat:1937","revision":2},"owners":[{"id":"own_hist_1","agent_id":"hist-agent","claimed_at":"2026-01-10T12:00:00Z","claim_type":"primary"}],"dependencies":[],"tags":["compat","hardening"],"verification":{"proof_plan":"[\"validate_passes\"]","verification_artifacts":"{\"task_id\":\"feat_hist1937compat01\",\"proof_plan_results\":[{\"proof_gate\":\"validate_passes\",\"output_hash\":\"sha256:historical1937\"}]}","last_verified_at":"2026-01-10T12:05:00Z","last_verified_status":"passed","last_verified_notes":"Original historical verification notes stay immutable.","verification_policy_days":90,"updated_at":"2026-01-10T12:05:00Z"}},"projection_digest":"f751949ae363847f3ff9b1bd2eff8056e1e814e6944424c25b6555479e2f23a9","record_digest":"a6d21a7d5d8b5c77c091fc31a9f409f37eb7bdc9d8079288d26e6e26e09d174c"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_COMMENT_1','2026-01-10T12:10:00Z',5,'todo','task','feat_hist1937compat01','task.comment','{"event_id":"EV_V1_COMMENT_1","ts":"2026-01-10T12:10:00Z","event_type":"task.comment","actor":"hist-agent","task_id":"feat_hist1937compat01","status":"success","payload":{"comment":"Historical pre-codec comment"}}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_BROKER_1','2026-01-10T12:00:00Z',1,'broker',NULL,NULL,'broker.audit','{"event_id":"EV_V1_BROKER_1","ts":"2026-01-10T12:00:00Z","event_type":"broker.audit","actor":"hist-agent","actor_id":"hist-agent","status":"success","schema_version":"1.1.0","db_id":"decapod.db","session_id":null,"correlation_id":null,"causation_id":null,"idempotency_key":null,"intent_ref":null,"request_id":"","caller_agent_id":"","op":"todo.add"}','hist-agent');
INSERT INTO events(event_id,ts,seq,stream,subject_kind,subject_id,event_type,payload,actor) VALUES ('EV_V1_FED_1','2026-01-10T12:00:00Z',1,'federation','node','node_hist_1','node.create','{"node_id":"node_hist_1","title":"Historical federation node","body":"Pre-codec federation context"}','hist-agent');
INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,last_shown_at,created_at,updated_at) VALUES ('p_dup_1','compat','Always preserve proofs.',100,1,2,'2026-01-10T12:00:00Z','2026-01-01T00:00:00Z','2026-01-10T12:00:00Z');
INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,last_shown_at,created_at,updated_at) VALUES ('p_dup_2','compat','Always preserve proofs.',100,1,3,'2026-01-11T12:00:00Z','2026-01-02T00:00:00Z','2026-01-11T12:00:00Z');
INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,last_shown_at,created_at,updated_at) VALUES ('p_uniq','other','Unique prompt.',50,1,1,NULL,'2026-01-03T00:00:00Z',NULL);
INSERT INTO summaries(summary_hash,ts,scope,original_hashes,summary_text,token_estimate) VALUES ('sum_hist_1','2026-01-10T12:00:00Z','repo','["h1","h2"]','Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. Historical cold LCM summary line for compaction qualification. ',256);
