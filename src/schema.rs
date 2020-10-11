table! {
    contest (id) {
        id -> Integer,
        name -> Text,
        start_instant -> Nullable<Timestamp>,
        end_instant -> Nullable<Timestamp>,
    }
}

table! {
    contest_problems (id) {
        id -> Integer,
        label -> Text,
        contest_id -> Integer,
        problem_id -> Text,
    }
}

table! {
    problem (id) {
        id -> Text,
        name -> Text,
        memory_limit_bytes -> Integer,
        time_limit_ms -> Integer,
        checker_path -> Text,
        checker_language -> Text,
        validator_path -> Text,
        validator_language -> Text,
        main_solution_path -> Text,
        main_solution_language -> Text,
        test_count -> Integer,
        status -> Text,
    }
}

table! {
    submission (uuid) {
        uuid -> Text,
        verdict -> Nullable<Text>,
        source_text -> Text,
        language -> Text,
        submission_instant -> Timestamp,
        judge_start_instant -> Nullable<Timestamp>,
        judge_end_instant -> Nullable<Timestamp>,
        memory_kib -> Nullable<Integer>,
        time_ms -> Nullable<Integer>,
        time_wall_ms -> Nullable<Integer>,
        error_output -> Nullable<Text>,
        contest_problem_id -> Integer,
        user_id -> Integer,
    }
}

table! {
    user (id) {
        id -> Integer,
        name -> Text,
        hashed_password -> Text,
        is_admin -> Bool,
    }
}

joinable!(contest_problems -> contest (contest_id));
joinable!(contest_problems -> problem (problem_id));
joinable!(submission -> contest_problems (contest_problem_id));
joinable!(submission -> user (user_id));

allow_tables_to_appear_in_same_query!(
    contest,
    contest_problems,
    problem,
    submission,
    user,
);
