-- A stored function is program text outside the program.
create function set_modified() returns trigger as $$
begin
    return new;
end;
$$ language plpgsql;
