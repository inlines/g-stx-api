// @generated automatically by Diesel CLI.

diesel::table! {
    alternative_names (id) {
        id -> Int4,
        product_id -> Int4,
        name -> Nullable<Text>,
        comment -> Nullable<Text>,
    }
}

diesel::table! {
    companies (id) {
        id -> Int4,
        changed_company_id -> Nullable<Int4>,
        start_date -> Nullable<Int8>,
        start_date_format -> Nullable<Int8>,
        status -> Nullable<Int4>,
        name -> Nullable<Text>,
        description -> Nullable<Text>,
        developed -> Nullable<Text>,
        published -> Nullable<Text>,
    }
}

diesel::table! {
    covers (id) {
        id -> Int4,
        image_url -> Text,
    }
}

diesel::table! {
    franschises (id) {
        id -> Int4,
        name -> Text,
    }
}

diesel::table! {
    game_bundles (id) {
        id -> Int4,
        member_id -> Int4,
        bundle_id -> Int4,
    }
}

diesel::table! {
    game_dlcs (id) {
        id -> Int4,
        main_id -> Int4,
        dlc_id -> Int4,
    }
}

diesel::table! {
    game_franschises (franschise_id, product_id) {
        franschise_id -> Int4,
        product_id -> Int4,
    }
}

diesel::table! {
    involved_companies (id) {
        id -> Int4,
        company -> Nullable<Int4>,
        game -> Nullable<Int4>,
        developer -> Nullable<Bool>,
        porting -> Nullable<Bool>,
        publisher -> Nullable<Bool>,
        supporting -> Nullable<Bool>,
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
        digital_only -> Bool,
    }
}

diesel::table! {
    products (id) {
        id -> Int4,
        name -> Text,
        summary -> Text,
        first_release_date -> Nullable<Int4>,
        cover_id -> Nullable<Int4>,
        total_rating -> Nullable<Float8>,
        game_type -> Nullable<Int4>,
        parent_game -> Nullable<Int4>,
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
        digital_only -> Bool,
        serial -> Nullable<Array<Nullable<Text>>>,
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
        price -> Nullable<Int4>,
    }
}

diesel::table! {
    users_have_wishes (release_id, user_login) {
        release_id -> Int4,
        user_login -> Text,
    }
}

diesel::joinable!(alternative_names -> products (product_id));
diesel::joinable!(game_franschises -> franschises (franschise_id));
diesel::joinable!(game_franschises -> products (product_id));
diesel::joinable!(product_platforms -> platforms (platform_id));
diesel::joinable!(product_platforms -> products (product_id));
diesel::joinable!(releases -> products (product_id));
diesel::joinable!(screenshots -> products (game));
diesel::joinable!(users_have_bids -> releases (release_id));
diesel::joinable!(users_have_releases -> releases (release_id));
diesel::joinable!(users_have_wishes -> releases (release_id));

diesel::allow_tables_to_appear_in_same_query!(
    alternative_names,
    companies,
    covers,
    franschises,
    game_bundles,
    game_dlcs,
    game_franschises,
    involved_companies,
    messages,
    platforms,
    product_platforms,
    products,
    regions,
    releases,
    screenshots,
    users,
    users_have_bids,
    users_have_releases,
    users_have_wishes,
);
