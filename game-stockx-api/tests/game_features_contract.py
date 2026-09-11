"""Feature behaviour in the admin suite's disposable databases."""
def exercise(request,sql):
    sql("""INSERT INTO products(id,name,summary,total_rating) VALUES(2,'Rated higher','',95),(3,'Unrated','',NULL);
      INSERT INTO platforms(id,name) VALUES(167,'PS5');
      INSERT INTO product_platforms(product_id,platform_id,digital_only) VALUES(2,48,false),(3,48,false),(1,167,false);
      UPDATE products SET total_rating=80,total_rating_count=25,similar_game_ids=ARRAY[2,99999,1] WHERE id=1;
      INSERT INTO product_multiplayer_modes(id,game,platform,offlinemax,onlinemax,offlinecoop) VALUES(1,1,48,4,8,true),(2,2,167,2,NULL,true),(3,3,48,NULL,16,NULL);
      UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;""")
    def catalogue(query):return request('/api/products?cat=48&limit=15&offset=0&'+query)
    results=catalogue('sort=rating')
    assert [p['id'] for p in results['items']]==[2,1,3]
    game=next(p for p in results['items'] if p['id']==1)
    assert (game['local_players'],game['online_players'],game['total_rating_count'])==(4,8,25)
    assert catalogue('local_multiplayer=true')['total_count']==1
    assert [p['id'] for p in catalogue('local_multiplayer=true')['items']]==[1]
    assert catalogue('online_multiplayer=true')['total_count']==2
    assert catalogue('local_multiplayer=true&online_multiplayer=true')['total_count']==1
    ps5=request('/api/products?cat=167&limit=15&offset=0&local_multiplayer=true')
    assert ps5['total_count']==0, 'PS4 multiplayer must not leak into PS5'
    details=request('/api/products/1')
    assert details['product']['total_rating']==80
    assert details['multiplayer'][0]['local_players']==4
    assert [g['id'] for g in details['similar_games']]==[2]
    assert details['similar_games'][0]['platform_ids']==[48]
    sql("DELETE FROM product_multiplayer_modes; DELETE FROM product_platforms WHERE platform_id=167; DELETE FROM platforms WHERE id=167; DELETE FROM products WHERE id IN (2,3); UPDATE products SET total_rating=NULL,total_rating_count=NULL,similar_game_ids='{}' WHERE id=1; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;")
    print('PASS: platform-specific multiplayer counts/filter intersections, rating sort with null last, existing-only similar links and details')
