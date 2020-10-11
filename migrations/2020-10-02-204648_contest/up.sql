CREATE TABLE contest (
  id integer primary key autoincrement not null,
  name text not null,
  start_instant timestamp null,
  end_instant timestamp null
)
