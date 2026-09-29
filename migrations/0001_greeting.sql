-- The worked-example table. Conventions every migration here follows (scripts/check-migrations.mjs and squawk
-- enforce the mechanical ones):
--   * timeouts first: a migration that waits on a lock or runs long fails fast instead of stalling a deploy.
--   * primary key: uuid, DEFAULT uuidv7() (PostgreSQL 18 native) as the backstop for ad-hoc SQL; the app
--     assigns v7 ids itself through platform::ids::new_id. Never serial, identity, sequences or gen_random_uuid().
--   * timestamps: timestamptz, never timestamp; no clock function as a column default, the app's Clock is the
--     only clock.
--   * a version column makes every UPDATE of the table the guarded update (db::versioned, scripts/check-sql.mjs).
--   * nothing here is ever edited after it ships (scripts/check-applied-migrations.mjs); a change is a new file.
set lock_timeout = '5s';
set statement_timeout = '60s';

create table greeting (
    id         uuid primary key default uuidv7(),
    name       text not null check (char_length(name) between 1 and 100),
    created_at timestamptz not null,
    version    bigint not null
);

create index greeting_created_at_id_idx on greeting (created_at desc, id desc);
