
def exercise(request, sql, admin, ordinary):
    serial='/api/admin/releases/1/serials'
    names='/api/admin/products/1/alternative-names'
    before=sql('SELECT (SELECT count(*) FROM release_serial_requests),(SELECT count(*) FROM kudos_awards)')
    for token,status in [(None,401),('bad',401),(ordinary,403)]:
        request(serial,token,data={'serial':'CUSA-00987'},status=status)
        request(names,token,data={'name':'Another name'},status=status)
    request(serial,admin,data={'serial':' '},status=400)
    request(names,admin,data={'name':''},status=400)
    request('/api/admin/releases/999999/serials',admin,data={'serial':'CUSA-00987'},status=404)
    request('/api/admin/products/999999/alternative-names',admin,data={'name':'Alias'},status=404)
    sql("INSERT INTO platforms(id,name) VALUES(7,'PS1'); INSERT INTO releases(id,product_id,platform,release_region) VALUES(900,1,7,1);")
    request('/api/admin/releases/900/serials',admin,data={'serial':'SLUS-00111'},status=400)
    # Cache the game and empty alias search before changing either field.
    request('/api/products/1',admin)
    request('/api/products?cat=48&limit=15&offset=0&query=DirectAlias',admin)
    request(serial,admin,data={'serial':' cusa00987 '},status=204)
    request(serial,admin,data={'serial':'CUSA-00987'},status=409)
    assert sql('SELECT serial[1] FROM releases WHERE id=1')=='CUSA-00987'
    request(names,admin,data={'name':'  DirectAlias   日本語  '},status=204)
    request(names,admin,data={'name':'directalias 日本語'},status=409)
    request(names,admin,data={'name':'Fixture'},status=409)
    assert 'DirectAlias 日本語' in request('/api/products/1',admin)['product']['alternative_names']
    assert request('/api/products?cat=48&limit=15&offset=0&query=DirectAlias',admin)['total_count']==1
    # Use a rollback failure after the write to prove both cache revision and data are atomic.
    revision=sql('SELECT revision FROM catalog_cache_revision')
    sql("CREATE FUNCTION direct_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'rollback'; END $$; CREATE TRIGGER direct_failure BEFORE UPDATE ON catalog_cache_revision FOR EACH ROW EXECUTE FUNCTION direct_failure();")
    request(serial,admin,data={'serial':'CUSA-90001'},status=500)
    sql('DROP TRIGGER direct_failure ON catalog_cache_revision; DROP FUNCTION direct_failure();')
    assert sql('SELECT revision FROM catalog_cache_revision')==revision
    assert sql("SELECT count(*) FROM releases WHERE 'CUSA-90001'=ANY(serial)")=='0'
    assert sql('SELECT (SELECT count(*) FROM release_serial_requests),(SELECT count(*) FROM kudos_awards)')==before
    sql("DELETE FROM releases WHERE id=900; DELETE FROM platforms WHERE id=7; DELETE FROM alternative_names WHERE name='DirectAlias 日本語'; UPDATE releases SET serial=NULL WHERE id=1; UPDATE products SET cache_revision=cache_revision+1 WHERE id=1; UPDATE catalog_name_revision SET revision=revision+1; UPDATE catalog_cache_revision SET revision=revision+1;")
    print('PASS: direct admin changes, auth/role/console restrictions, duplicates, normalized Unicode, cache freshness, rollback, no requests/photos/Kudos')
