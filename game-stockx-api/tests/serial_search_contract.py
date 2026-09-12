"""Exact serial catalogue searches, platform/region scoping and canonical writes."""
from urllib.parse import urlencode

def exercise(request, sql):
    sql("""INSERT INTO platforms(id,name,abbreviation) VALUES(167,'PS5','PS5');
      INSERT INTO regions(id,name) VALUES(2,'North America'),(8,'Worldwide') ON CONFLICT DO NOTHING;
      INSERT INTO products(id,name,summary,first_release_date) VALUES(700,'Serial fixture','',1500000000),(701,'Serial other','',1500000000);
      INSERT INTO product_platforms VALUES(700,48,false),(700,167,false),(701,48,false);
      INSERT INTO releases(id,product_id,platform,release_region,serial) VALUES
        (700,700,48,1,ARRAY[' cusa10001 ','CUSA-10001/H/ITA']),
        (701,700,48,2,ARRAY['CUSA-10002']),
        (702,700,167,1,ARRAY['PPSA-10003']),
        (703,701,48,8,ARRAY['CUSA-10004']);
      UPDATE catalog_cache_revision SET revision=revision+1;
    """)
    def search(code,region='',cat=48,**kw):
        return request('/api/products?'+urlencode(dict(cat=cat,limit=15,query=code,regions=region,search_mode='serial')),**kw)
    def ids(code,region='',cat=48):
        result=search(code,region,cat); assert result['total_count']==len(result['items']); return [p['id'] for p in result['items']]
    assert sql('SELECT serial[1] FROM releases WHERE id=700')=='CUSA-10001'
    assert ids('cusa10001')==[700]
    assert ids('CUSA-10001','europe')==[700]
    assert ids('CUSA-10001','america')==[]
    assert ids('CUSA-10001','',167)==[]
    assert ids('PPSA-10003')==[]
    assert ids('PPSA-10003','europe',167)==[700]
    assert ids('CUSA-10001/H/ITA')==[700]
    assert ids('CUSA-10001/H/POL')==[]
    for region in ['europe','america','japan','other']:
        assert ids('CUSA-10004',region)==[701]
    for invalid in ['CUSA-1000','CUSA-100010','%','CUSA-10001,CUSA-10002']:
        search(invalid,status=400)
    assert search('CUSA-10002','america')['items'][0]['serial']==['CUSA-10002']
    assert search('CUSA-10001')['items'][0]['serial']==[]
    assert search('CUSA-10001','europe')['items'][0]['serial']==['CUSA-10001','CUSA-10001/H/ITA']
    assert search('CUSA-10001','europe,america')['items'][0]['serial']==['CUSA-10001','CUSA-10001/H/ITA','CUSA-10002']
    assert search('CUSA-10004','europe')['items'][0]['serial']==[]
    assert search('CUSA-10004','other')['items'][0]['serial']==['CUSA-10004']
    assert search('PPSA-10003','europe',167)['items'][0]['serial']==['PPSA-10003']
    assert request('/api/products?cat=48&limit=15&query=Serial%20fixture')['total_count']==1
    assert request('/api/products?cat=48&limit=15&search_mode=serial')['total_count']>=2
    request('/api/products?cat=48&search_mode=bad',status=400)
    sql("DELETE FROM products WHERE id IN (700,701); DELETE FROM platforms WHERE id=167; DELETE FROM regions WHERE id IN (2,8); UPDATE catalog_cache_revision SET revision=revision+1;")
    print('PASS: exact canonical serial search, suffixes, platform/region isolation, Worldwide, priority and invalid input')
