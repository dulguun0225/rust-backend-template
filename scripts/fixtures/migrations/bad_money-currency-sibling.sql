create table t (
    id uuid primary key default uuidv7(),
    total_amount numeric(19,4) not null check (total_amount <> 'NaN')
);
