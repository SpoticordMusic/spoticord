// @generated automatically by Diesel CLI.

diesel::table! {
    account (user_id) {
        user_id -> Varchar,
        #[max_length = 64]
        username -> Varchar,
        #[max_length = 1024]
        access_token -> Varchar,
        #[max_length = 1024]
        refresh_token -> Varchar,
        #[max_length = 1024]
        session_token -> Nullable<Varchar>,
        expires -> Timestamp,
        last_updated -> Timestamp,
    }
}

diesel::table! {
    link_request (token) {
        token -> Text,
        user_id -> Text,
        expires -> Timestamp,
    }
}

diesel::table! {
    user (id) {
        id -> Varchar,
        #[max_length = 32]
        device_name -> Varchar,
    }
}

diesel::joinable!(account -> user (user_id));
diesel::joinable!(link_request -> user (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    account,
    link_request,
    user,
);
