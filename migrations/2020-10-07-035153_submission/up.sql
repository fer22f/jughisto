CREATE TABLE submission (
  uuid text primary key not null,
  verdict text null,
  source_text text not null,
  language text not null,
  submission_instant timestamp not null,
  judge_start_instant timestamp null,
  judge_end_instant timestamp null,
  memory_kib integer null,
  time_ms integer null,
  time_wall_ms integer null,
  error_output text null,
  contest_problem_id integer references contest_problems(id) not null,
  user_id integer references "user"(id) not null
)
