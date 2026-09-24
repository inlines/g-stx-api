"""Run with the isolated HTTP fixture harness: exercise(request, sql)."""
from datetime import datetime, timezone

def exercise(request, sql):
    now = datetime.now(timezone.utc)
    first = int(datetime(now.year, now.month, 1, tzinfo=timezone.utc).timestamp())
    month = now.month + 3
    end = int(datetime(now.year + (month-1)//12, (month-1)%12+1, 1, tzinfo=timezone.utc).timestamp())
    sql(f"""
    INSERT INTO regions(id,name) VALUES(2,'north_america') ON CONFLICT DO NOTHING;
    INSERT INTO platforms(id,name,abbreviation) VALUES(167,'PS5','PS5'),(9,'PS3','PS3') ON CONFLICT DO NOTHING;
    INSERT INTO products(id,name,summary,game_type) SELECT i,'Calendar fixture '||i,'',0 FROM generate_series(700001,700006) i;
    INSERT INTO releases(id,product_id,platform,release_region,release_date,release_status) VALUES
    (700001,700001,48,1,{first},NULL),(700002,700001,48,2,{first},NULL),
    (700003,700001,167,1,{first},NULL),(700004,700002,48,1,{end-1},NULL),
    (700005,700003,48,1,{end},NULL),(700006,700004,48,1,{first-1},NULL),
    (700007,700005,48,1,{first},5),(700008,700006,9,1,{first},NULL);
    """)
    result=request('/api/release-calendar')
    assert result['start']==now.strftime('%Y-%m-01')
    events=[e for e in result['items'] if e['id']>=700001]
    assert len(events)==3,events
    assert {(e['id'],e['platform']) for e in events}=={(700001,48),(700001,167),(700002,48)}
    assert all(e['day']>=result['start'] for e in events)
    print('PASS calendar: month bounds, PS4/PS5 scope, cancelled exclusion, region deduplication and separate platforms')
