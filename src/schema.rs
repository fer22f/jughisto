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
    user (id) {
        id -> Integer,
        name -> Text,
        hashed_password -> Text,
        is_admin -> Bool,
    }
}

joinable!(problem -> contest (contest_id));

allow_tables_to_appear_in_same_query!(
    contest,
    problem,
    user,
);
