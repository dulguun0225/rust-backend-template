-- A migration squawk must refuse: a blocking index build and a NOT NULL column without a default.
set lock_timeout = '5s';
set statement_timeout = '60s';
create index greeting_name_idx on greeting (name);
alter table greeting add column nickname text not null;
