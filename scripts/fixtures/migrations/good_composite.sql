-- composite-key: a pure child row; (invoice_id, line_no) is its identity
create table invoice_line (
    invoice_id uuid not null,
    line_no    integer not null,
    primary key (invoice_id, line_no)
);
