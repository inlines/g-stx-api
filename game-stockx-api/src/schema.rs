// @generated automatically by Diesel CLI.

diesel::table! {
    covers (id) {
        id -> Int4,
        image_url -> Text,
    }
}

diesel::table! {
    messages (id) {
        id -> Int4,
        sender_login -> Text,
        recipient_login -> Text,
        body -> Text,
        created_at -> Timestamptz,
        read -> Bool,
    }
}

diesel::table! {
    platforms (id) {
        id -> Int4,
        abbreviation -> Nullable<Text>,
        name -> Text,
        generation -> Nullable<Int4>,
        active -> Nullable<Bool>,
        total_games -> Nullable<Int4>,
    }
}

diesel::table! {
    product_platforms (product_id, platform_id) {
        product_id -> Int4,
        platform_id -> Int4,
    }
}

diesel::table! {
    products (id) {
        id -> Int4,
        name -> Text,
        summary -> Text,
        first_release_date -> Nullable<Int4>,
        cover_id -> Nullable<Int4>,
    }
}

diesel::table! {
    regions (id) {
        id -> Int4,
        name -> Text,
    }
}

diesel::table! {
    releases (id) {
        id -> Int4,
        release_date -> Nullable<Int4>,
        product_id -> Int4,
        platform -> Int4,
        release_status -> Nullable<Int4>,
        release_region -> Nullable<Int4>,
    }
}

diesel::table! {
    sales (id) {
        id -> Int4,
        created_at -> Timestamp,
        product_id -> Int4,
        total_price -> Int4,
    }
}

diesel::table! {
    screenshots (id) {
        id -> Int4,
        image_url -> Text,
        game -> Int4,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        user_login -> Text,
        password_hash -> Text,
        created_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    users_have_bids (release_id, user_login) {
        release_id -> Int4,
        user_login -> Text,
    }
}

diesel::table! {
    users_have_releases (release_id, user_login) {
        release_id -> Int4,
        user_login -> Text,
    }
}

diesel::table! {
    users_have_wishes (release_id, user_login) {
        release_id -> Int4,
        user_login -> Text,
    }
}

diesel::joinable!(product_platforms -> platforms (platform_id));
diesel::joinable!(product_platforms -> products (product_id));
diesel::joinable!(releases -> products (product_id));
diesel::joinable!(screenshots -> products (game));
diesel::joinable!(users_have_bids -> releases (release_id));
diesel::joinable!(users_have_releases -> releases (release_id));
diesel::joinable!(users_have_wishes -> releases (release_id));

diesel::allow_tables_to_appear_in_same_query!(
    covers,
    messages,
    platforms,
    product_platforms,
    products,
    regions,
    releases,
    sales,
    screenshots,
    users,
    users_have_bids,
    users_have_releases,
    users_have_wishes,
);
