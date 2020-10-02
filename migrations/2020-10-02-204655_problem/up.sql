CREATE TABLE problem (
  id integer primary key autoincrement not null,
  label text not null,
  contest_id integer not null references contest(id),
  name text not null
)
