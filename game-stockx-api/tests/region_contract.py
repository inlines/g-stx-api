"""Region filters and distinct non-digital platform totals on disposable PostgreSQL."""
def exercise(request, sql, admin_request, nonadmin_request):
    sql("""UPDATE platforms SET active=true,total_games=9999 WHERE id=48;
      INSERT INTO platforms(id,name,abbreviation,active) VALUES(167,'PS5','PS5',true);
      INSERT INTO regions(id,name) VALUES(2,'north_america'),(5,'japan'),(8,'worldwide'),(10,'brazil') ON CONFLICT DO NOTHING;
      INSERT INTO products(id,name,summary,first_release_date) SELECT id,'Region game '||id,'',1500000000 FROM generate_series(300,305) id;
      INSERT INTO product_platforms(product_id,platform_id,digital_only) SELECT id,48,id=304 FROM generate_series(300,305) id;
      INSERT INTO product_platforms VALUES(305,167,false);
      INSERT INTO releases(id,product_id,platform,release_region,digital_only) VALUES
        (300,300,48,1,false),(301,300,48,1,false),(302,300,48,2,false),
        (303,301,48,10,false),(304,302,48,NULL,false),(305,303,48,8,true),
        (306,304,48,1,false),(307,305,48,5,false),(308,305,167,1,false);
      INSERT INTO users_have_releases(user_login,release_id) VALUES('ordinary',300),('ordinary',302),('ordinary',304),('ordinary',305);
      UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;""")
    def catalog(regions):return request('/api/products?cat=48&limit=15&offset=0&ignore_digital=true&regions='+regions)
    def ids(regions):return {p['id'] for p in catalog(regions)['items'] if p['id']>=300}
    assert ids('europe')=={300}, 'European PS5 release must not leak into PS4'
    assert ids('america')=={300}, 'Brazil belongs to Other'
    assert ids('japan')=={305}
    assert ids('japan,america')=={300,305}
    assert ids('other')=={301,302,303}, 'Unknown and worldwide included in Other'
    assert ids('europe,america')=={300}
    assert catalog('europe,america')['total_count']==2, 'Duplicate releases must not duplicate games'
    assert ids('')=={300,301,302,303,305}
    assert catalog('america,europe,europe')==catalog('europe,america')
    request('/api/products?cat=48&regions=invalid',status=400)
    counts=next(p for p in request('/api/platforms') if p['id']==48)
    assert (counts['total_games'],counts['europe_games'],counts['america_games'],counts['japan_games'],counts['other_games'])==(5,2,1,1,2),counts
    unknown_url='/api/products?cat=48&limit=15&ignore_digital=true&unknown=true'
    sql("UPDATE releases SET serial=ARRAY['KNOWN'] WHERE id=300; UPDATE releases SET serial=ARRAY['PS5-ONLY'] WHERE id=308; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    unknown=admin_request(unknown_url)
    assert all(not p['has_serials'] for p in unknown['items'])
    assert {p['id'] for p in unknown['items'] if p['id']>=300}=={301,302,303,305}
    assert unknown['total_count']==len(unknown['items'])
    assert [p['id'] for p in admin_request(unknown_url+'&regions=japan')['items']]==[305]
    assert admin_request(unknown_url+'&regions=america')['total_count']==0
    assert admin_request(unknown_url+'&query=Region%20game%20301')['total_count']==1
    # Authorization is also enforced on a warmed cache.
    nonadmin_request(unknown_url,status=403)
    assert 300 in ids(''), 'Unknown mode must not affect the ordinary catalogue'
    sql("UPDATE releases SET serial=ARRAY['IDENTIFIED'] WHERE id=307; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    assert admin_request(unknown_url+'&regions=japan')['total_count']==0
    # Same endpoint works for owned releases without cover or region metadata.
    own=request('/api/collection?cat=48&limit=15&offset=0')
    by_id={p['release_id']:p for p in own['items']}
    assert own['total_count']==len(by_id)==5
    assert by_id[304]['region_id'] is None and by_id[304]['region_name'] is None
    assert by_id[305]['digital_only'] is True
    assert by_id[300]['platform_id']==48 and by_id[300]['region_id']==1
    public={p['release_id']:p for p in request('/api/collection-by-login/ordinary?cat=0&limit=1000')}
    assert public[304]['region_id'] is None and all(p['price'] is None for p in public.values())
    sql("UPDATE releases SET digital_only=true WHERE id=303; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    counts=next(p for p in request('/api/platforms') if p['id']==48)
    assert counts['other_games']==1 and counts['total_games']==4, 'Revision invalidates regional totals'
    sql("DELETE FROM products WHERE id BETWEEN 300 AND 305; DELETE FROM platforms WHERE id=167; DELETE FROM regions WHERE id IN (2,5,8,10); UPDATE platforms SET active=false WHERE id=48; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    print('PASS: region union and platform isolation, Brazil/unknown mapping, unique digital-free totals, complete collection metadata and cache invalidation')
