UPDATE tasks
SET parent_task_id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = tasks.parent_task_id
)
WHERE parent_task_id IN (SELECT old_id FROM task_id_migration_map);

UPDATE task_verification
SET todo_id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = task_verification.todo_id
)
WHERE todo_id IN (SELECT old_id FROM task_id_migration_map);

UPDATE task_owners
SET task_id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = task_owners.task_id
)
WHERE task_id IN (SELECT old_id FROM task_id_migration_map);

UPDATE task_dependencies
SET task_id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = task_dependencies.task_id
)
WHERE task_id IN (SELECT old_id FROM task_id_migration_map);

UPDATE task_dependencies
SET depends_on_task_id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = task_dependencies.depends_on_task_id
)
WHERE depends_on_task_id IN (SELECT old_id FROM task_id_migration_map);

UPDATE task_events
SET task_id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = task_events.task_id
)
WHERE task_id IN (SELECT old_id FROM task_id_migration_map);

UPDATE tasks
SET id = (
    SELECT m.new_id
    FROM task_id_migration_map m
    WHERE m.old_id = tasks.id
)
WHERE id IN (SELECT old_id FROM task_id_migration_map);
