-- A trigger is behaviour no source rule can see.
create table triggered_thing (
    id          uuid primary key default uuidv7(),
    modified_at timestamptz not null
);

create trigger triggered_thing_touch before update on triggered_thing
    for each row execute function set_modified();
