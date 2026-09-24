"""Catalogue visibility against real PostgreSQL and Redis, with isolated fixtures."""
from urllib.parse import urlencode


def exercise(request, sql):
    sql("""
      INSERT INTO platforms(id,name,abbreviation) VALUES(167,'PS5','PS5') ON CONFLICT DO NOTHING;
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
        (209,'Visibility 100%_game','',1500000000,0,NULL),
        (210,'Visibility platform date only','',NULL,0,NULL),
        (211,'Visibility Super Mario War analogue','',1500000000,0,NULL),
        (212,'Visibility future','',2100000000,0,NULL),
        (213,'Visibility Cancelled planned date','',1500000000,0,NULL),
        (214,'Visibility Cancelled serial','',1500000000,0,NULL),
        (215,'Visibility Cancelled PS4 released PS5','',1500000000,0,NULL),
        (216,'Visibility mixed release statuses','',1500000000,0,NULL);
      INSERT INTO product_platforms(product_id,platform_id,digital_only)
        SELECT id,48,id=202 FROM products WHERE id BETWEEN 200 AND 216;
      INSERT INTO releases(id,product_id,platform,release_region,serial,digital_only) VALUES
        (200,200,48,1,ARRAY['TEST-200'],false),
        (202,202,48,1,ARRAY['TEST-202'],false),
        (203,203,167,1,ARRAY['TEST-203'],false),
        (204,204,48,1,ARRAY['TEST-204'],false),
        (205,205,48,1,ARRAY[NULL,'','   '],false),
        (206,206,48,1,ARRAY['TEST-206'],true),
        (207,207,48,1,ARRAY['TEST-207'],false),
        (209,209,48,1,NULL,false),
        (210,210,48,1,NULL,false),
        (211,211,48,1,ARRAY[NULL,'','  '],false),
        (212,211,167,1,NULL,false);
      UPDATE releases SET release_date=1500000000 WHERE id IN (200,202,203,206,209,210,212);
      UPDATE releases SET release_status=6 WHERE id=204;
      INSERT INTO product_platforms(product_id,platform_id,digital_only) VALUES(215,167,false);
      INSERT INTO releases(id,product_id,platform,release_region,release_date,release_status,serial) VALUES
        (213,213,48,1,1500000000,5,NULL),
        (214,214,48,1,NULL,5,ARRAY['CUSA-00214']),
        (215,215,48,1,1500000000,5,NULL),
        (216,215,167,1,1500000000,6,NULL),
        (217,216,48,1,1400000000,5,NULL),
        (218,216,48,1,1500000000,6,NULL),
        (219,212,48,1,2100000000,6,NULL),
        (220,209,48,5,1500000000,5,NULL);
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

    expected = {200, 206, 209, 210, 216}
    extended = expected | {201, 203, 204, 205, 211, 212, 213, 214, 215}
    assert ids() == expected, 'Require a non-cancelled dated release on the selected platform'
    assert ids(query='Visibility') == expected, 'Search must respect visibility and game-type exclusions'
    assert ids(query='Secret alias') == set()
    assert ids(query='Secret alias', include_unreleased='true') == {201}
    assert ids(query='undated') == set(), 'An existing serial must not bypass the missing date'
    assert ids(query='undated', include_unreleased='true') == {204}
    assert ids(query='Super Mario War') == set(), 'A date on another platform must not leak'
    assert ids(query='Super Mario War', include_unreleased='true') == {211}
    assert ids(query='Cancelled') == set(), 'Cancelled dates and even serials must not bypass the gate'
    assert ids(query='Cancelled', include_unreleased='true') == {213, 214, 215}
    assert ids(cat=167, query='Cancelled') == {215}, 'Cancellation on PS4 must not hide a released PS5 version'
    assert 216 in ids(), 'A cancelled release must not hide another valid release on the same platform'
    assert ids(include_unreleased='true') == extended
    assert ids(include_unreleased='false') == expected, 'Cached inclusion must not leak'
    assert ids(ignore_digital='false') == expected | {202}
    assert ids(ignore_digital='false', query='Visibility') == expected | {202}
    assert next(p for p in catalog()['items'] if p['id'] == 206)['has_serials'] is True
    for include in ['false', 'true']:
        all_visible = catalog(include_unreleased=include)
        assert all_visible['total_count'] == len(all_visible['items'])
        page = catalog(limit=1, offset=1, include_unreleased=include)
        assert page['total_count'] == all_visible['total_count']
        assert page['items'] == all_visible['items'][1:2]
    assert ids(unknown='true') == {209, 210, 216}
    for item in catalog(include_unreleased='true')['items']:
        assert item['is_released'] == (item['id'] in expected)

    # Unknown never includes unreleased games, even with an explicit opt-in.
    assert ids(unknown='true', include_unreleased='true') == {209, 210, 216}
    platform = next(p for p in request('/api/platforms') if p['id'] == 48)
    totals = catalog(unknown='true')['region_totals']
    for region in ['europe','america','japan','other']:
        assert platform[region + '_games'] == totals[region], (platform, totals)
    assert catalog(unknown='true', query='100%_game')['region_totals']['japan'] == 0
    assert catalog(unknown='true', query='100%_game')['region_counts']['japan'] == 0
    for include in ['false', 'true']:
        unknown = catalog(unknown='true', include_unreleased=include)
        assert unknown['total_count'] == len(unknown['items'])
        for region in ['europe', 'america', 'japan', 'other']:
            assert unknown['region_counts'][region] == catalog(unknown='true', include_unreleased=include, regions=region)['total_count']
    # Adding a serial alone must not reveal the game; filling the date does.
    sql("INSERT INTO releases(id,product_id,platform,release_region,serial) VALUES(201,201,48,1,ARRAY['TEST-201']); UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    assert ids() == expected
    sql("UPDATE releases SET release_date=1500000000 WHERE id=201; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    assert ids() == expected | {201}
    assert next(p for p in catalog()['items'] if p['id'] == 201)['has_serials'] is True
    sql("DELETE FROM alternative_names WHERE id=99999; DELETE FROM game_bundles WHERE bundle_id=200; DELETE FROM products WHERE id BETWEEN 200 AND 216; DELETE FROM platforms WHERE id=167; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    print('PASS: strict platform date/cancellation gate (serials never bypass), mixed release statuses, blank serials, unknown game date, alias search, checkbox/cache separation and pagination')
