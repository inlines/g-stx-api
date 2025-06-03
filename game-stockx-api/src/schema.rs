// @generated automatically by Diesel CLI.

diesel::table! {
    covers (id) {
        id -> Int4,
        image_url -> Text,
    }
}

diesel::table! {
    platforms (id) {
        id -> Int4,
        abbreviation -> Nullable<Text>,
        name -> Text,
        generation -> Nullable<Int4>,
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

diesel::joinable!(releases -> products (product_id));
diesel::joinable!(sales -> products (product_id));

diesel::allow_tables_to_appear_in_same_query!(
    covers,
    platforms,
    products,
    regions,
    releases,
    sales,
);
