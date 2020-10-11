CREATE TABLE contest (
  id integer primary key autoincrement not null,
  name text not null,
  start_instant timestamp null,
  end_instant timestamp null,
  creation_user_id integer references user(id) not null,
  creation_instant timestamp not null
)
