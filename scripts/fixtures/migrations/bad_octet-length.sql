-- Measures a text column in bytes; lengths are characters.
create table byte_measured_thing (
    id   uuid primary key default uuidv7(),
    name text not null check (octet_length(name) <= 100)
);
