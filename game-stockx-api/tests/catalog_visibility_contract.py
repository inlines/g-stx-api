"""Catalogue visibility against real PostgreSQL and Redis, with isolated fixtures."""
from urllib.parse import urlencode


def exercise(request, sql):
    sql("""
      INSERT INTO platforms(id,name) VALUES(167,'PS5') ON CONFLICT DO NOTHING;
      INSERT INTO products(id,name,summary,first_release_date,game_type,parent_game) VALUES
        (200,'Visibility disc','',1500000000,3,NULL),
        (201,'Visibility unknown','',1500000000,0,NULL),
        (202,'Visibility digital','',1500000000,0,NULL),
        (203,'Visibility other platform','',1500000000,0,NULL),
        (204,'Visibility undated','',NULL,0,NULL),
        (205,'Visibility empty serial','',1500000000,0,NULL),
        (206,'Visibility digital release','',1500000000,0,NULL),
        (207,'Visibility physical child','',1500000000,1,200),
        (208,'Visibility unknown child','',1500000000,1,200),
        (209,'Visibility 100%_game','',1500000000,0,NULL);
      INSERT INTO product_platforms(product_id,platform_id,digital_only)
        SELECT id,48,id=202 FROM products WHERE id BETWEEN 200 AND 209;
      INSERT INTO releases(id,product_id,platform,release_region,serial,digital_only) VALUES
        (200,200,48,1,ARRAY['TEST-200'],false),
        (202,202,48,1,ARRAY['TEST-202'],false),
        (203,203,167,1,ARRAY['TEST-203'],false),
        (204,204,48,1,ARRAY['TEST-204'],false),
        (205,205,48,1,ARRAY[NULL,'','   '],false),
        (206,206,48,1,ARRAY['TEST-206'],true),
        (207,207,48,1,ARRAY['TEST-207'],false);
      INSERT INTO alternative_names(id,product_id,name) VALUES(99999,201,'Secret alias');
      INSERT INTO game_bundles(member_id,bundle_id) VALUES(207,200),(208,200);
      UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
    """)

    def catalog(**params):
        args = dict(cat=48, limit=15, offset=0, ignore_digital='true', sort='name')
        args.update(params)
        response = request('/api/products?' + urlencode(args))
        assert response['total_count'] >= len(response['items'])
        return response

    def ids(**params):
        return {p['id'] for p in catalog(**params)['items'] if p['id'] >= 200}

    expected = {200, 201, 203, 205, 206, 209}
    assert ids() == expected, 'Missing serials must not hide games; only platform digital_only excludes them'
    assert ids(query='Visibility') == expected, 'Search must not bypass the original game-type exclusions'
    assert ids(query='Secret alias') == {201}
    assert ids(query='undated') == set(), 'Search cannot bypass the release-date gate'
    assert ids(query='undated', include_unreleased='true') == {204}
    assert ids(include_unreleased='true') == expected | {204}
    assert ids(include_unreleased='false') == expected, 'Cached inclusion must not leak'
    assert ids(ignore_digital='false') == expected | {202}
    assert ids(ignore_digital='false', query='Visibility') == expected | {202}
    assert 204 not in ids(ignore_digital='false', query='Visibility')
    assert next(p for p in catalog()['items'] if p['id'] == 206)['has_serials'] is True
    all_visible = catalog()
    assert all_visible['total_count'] == len(all_visible['items'])
    page = catalog(limit=1, offset=1)
    assert page['total_count'] == all_visible['total_count']
    assert page['items'] == all_visible['items'][1:2]
    # Adding a serial updates the badge, not catalogue membership.
    sql("INSERT INTO releases(id,product_id,platform,release_region,serial) VALUES(201,201,48,1,ARRAY['TEST-201']); UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    assert ids() == expected
    assert next(p for p in catalog()['items'] if p['id'] == 201)['has_serials'] is True
    sql("DELETE FROM alternative_names WHERE id=99999; DELETE FROM game_bundles WHERE bundle_id=200; DELETE FROM products WHERE id BETWEEN 200 AND 209; DELETE FROM platforms WHERE id=167; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    print('PASS: restored digital filtering independent of serials, original game types and alias search, date gate, cache separation, counts and pagination')
