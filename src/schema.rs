table! {
    contest (id) {
        id -> Nullable<Integer>,
        name -> Text,
        start_instant -> Nullable<Integer>,
        end_instant -> Nullable<Integer>,
    }
}

table! {
    problem (id) {
        id -> Integer,
        label -> Text,
        contest_id -> Integer,
        name -> Text,
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
        compilation_stderr -> Nullable<Text>,
        problem_id -> Integer,
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

joinable!(problem -> contest (contest_id));
joinable!(submission -> problem (problem_id));
joinable!(submission -> user (user_id));

allow_tables_to_appear_in_same_query!(contest, problem, submission, user,);
