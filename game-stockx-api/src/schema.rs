// @generated automatically by Diesel CLI.

diesel::table! {
    covers (id) {
        id -> Int4,
        image_url -> Text,
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
    sales (id) {
        id -> Uuid,
        created_at -> Timestamp,
        product_id -> Int4,
        total_price -> Int4,
    }
}

diesel::joinable!(sales -> products (product_id));

diesel::allow_tables_to_appear_in_same_query!(
    covers,
    products,
    sales,
);
