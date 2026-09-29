fn f() { sqlx::query!("delete from audit where id = $1", id); }
