"""Called by admin_contract.py; uses only that test's disposable DB and API."""
import concurrent.futures
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


def exercise(base, admin, uploader, sql):
    jpeg = (Path(__file__).parent / 'fixtures/serial-proof.jpg').read_bytes()

    def request(path, token=admin, method='GET', body=None, status=200):
        time.sleep(.065)
        headers = {}
        if token: headers['Authorization'] = 'Bearer ' + token
        if body is not None: headers['Content-Type'] = 'image/jpeg'
        req = urllib.request.Request(base + path, method=method, headers=headers, data=body)
        try: response = urllib.request.urlopen(req, timeout=15)
        except urllib.error.HTTPError as error: response = error
        raw = response.read()
        assert response.status == status, (path, response.status, raw[:500])
        if response.headers.get_content_type() == 'image/jpeg': return raw
        try: return json.loads(raw)
        except ValueError: return raw

    def submit(release=1, serial='CUSA-12345', token=uploader, image=jpeg, status=201):
        return request(f'/api/releases/{release}/serial-requests?' + urllib.parse.urlencode({'serial': serial}), token, 'POST', image, status)

    sql("""UPDATE releases SET serial=ARRAY['OLD-123'] WHERE id=1;
        INSERT INTO platforms(id,name,abbreviation) VALUES(8,'PS2','PS2'),(7,'PlayStation','PS1'),(9,'PS3','PS3'),(38,'PSP','PSP'),(167,'PS5','PS5'),(6,'PC','PC');
        INSERT INTO regions(id,name) VALUES(2,'Asia');
        INSERT INTO releases(id,product_id,platform,release_region) VALUES(2,1,8,1),(3,1,9,1),(4,1,38,1),(5,1,167,1),(6,1,6,1),(7,1,48,2),(8,1,7,1);
    """)
    submit(token=None, status=401)
    submit(token='bad', status=401)
    submit(release=6, status=400)
    submit(release=8, status=400)  # PS1 must not accept serial requests.
    submit(release=999999, status=404)
    submit(serial='<script>', status=400)
    submit(serial='old-123', status=409)
    submit(image=b'not a jpeg', status=400)
    submit(image=b'0' * (768 * 1024 + 1), status=413)
    assert request('/api/kudos?login=victim', token=None)['kudos'] == 0
    entry = submit(serial='  cusa-12345  ')
    entry_id = entry['id']
    assert entry['status'] == 'pending'
    assert sql('SELECT serial::text FROM releases WHERE id=1') == '{OLD-123}'
    for token, code in [(None, 401), (uploader, 403)]:
        request('/api/admin/serial-requests', token, status=code)
        request(f'/api/admin/serial-requests/{entry_id}/photo', token, status=code)
        request(f'/api/admin/serial-requests/{entry_id}/accept', token, 'POST', status=code)
        request(f'/api/admin/serial-requests/{entry_id}', token, 'DELETE', status=code)
    submit(status=409)
    pending = request('/api/admin/serial-requests')['items'][0]
    assert pending['product_name'] == 'Fixture' and pending['release_id'] == 1
    assert pending['platform_id'] == 48 and pending['region_name'] == 'Europe'
    assert pending['serial'] == 'CUSA-12345' and pending['existing_serials'] == ['OLD-123']
    assert 'photo' not in pending
    assert request(f'/api/admin/serial-requests/{entry_id}/photo') == jpeg
    sql("UPDATE releases SET serial=array_append(serial,'EXTERNAL-456') WHERE id=1")
    assert request('/api/admin/serial-requests')['items'][0]['existing_serials'] == ['OLD-123','EXTERNAL-456']
    # Failure after updating the release must also roll back the serial append.
    sql("""CREATE FUNCTION test_block_review() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test rollback'; END $$;
        CREATE TRIGGER block_review BEFORE UPDATE ON release_serial_requests FOR EACH ROW EXECUTE FUNCTION test_block_review();""")
    request(f'/api/admin/serial-requests/{entry_id}/accept', method='POST', status=500)
    assert sql('SELECT serial::text FROM releases WHERE id=1') == '{OLD-123,EXTERNAL-456}'
    assert sql(f'SELECT status FROM release_serial_requests WHERE id={entry_id}') == 'pending'
    assert request('/api/kudos?login=victim')['kudos'] == 0
    sql('DROP TRIGGER block_review ON release_serial_requests; DROP FUNCTION test_block_review();')
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        first = executor.submit(request, f'/api/admin/serial-requests/{entry_id}/accept', admin, 'POST', None, 204)
        second = executor.submit(request, f'/api/admin/serial-requests/{entry_id}/accept', admin, 'POST', None, 204)
        first.result(); second.result()
    assert sql('SELECT serial::text FROM releases WHERE id=1') == '{OLD-123,EXTERNAL-456,CUSA-12345}'
    assert request('/api/admin/serial-requests')['total_count'] == 0
    assert request('/api/kudos?login=victim')['kudos'] == 10
    archive = request('/api/admin/serial-requests?status=accepted')['items'][0]
    assert archive['reviewed_at'] and archive['reviewer'] == 'segasanshiro'
    assert request(f'/api/admin/serial-requests/{entry_id}/photo') == jpeg
    request(f'/api/admin/serial-requests/{entry_id}', method='DELETE', status=409)
    product = request('/api/products/1', token=uploader)
    assert 'CUSA-12345' in next(r['serial'] for r in product['releases'] if r['release_id'] == 1)
    # NULL array, every allowed console, matching serial in another region.
    for release in [2,3,4,5,7]:
        item_id = submit(release=release)['id']
        if release == 7:
            item = request('/api/admin/serial-requests')['items'][0]
            assert item['region_name'] == 'Asia' and item['release_id'] == 7
        request(f'/api/admin/serial-requests/{item_id}/accept', method='POST', status=204)
        assert sql(f'SELECT serial::text FROM releases WHERE id={release}') == '{CUSA-12345}'
    # An external update between submission and approval must not duplicate a serial.
    another = submit(serial='CUSA-EXTERNAL')['id']
    sql("UPDATE releases SET serial=array_append(serial,'CUSA-EXTERNAL') WHERE id=1")
    request(f'/api/admin/serial-requests/{another}/accept', method='POST', status=204)
    assert sql("SELECT count(*) FROM releases,unnest(serial) s WHERE id=1 AND s='CUSA-EXTERNAL'") == '1'
    rejected = submit(serial='CUSA-REJECT')['id']
    request(f'/api/admin/serial-requests/{rejected}', method='DELETE', status=204)
    request(f'/api/admin/serial-requests/{rejected}/photo', status=404)
    assert sql(f'SELECT count(*) FROM release_serial_requests WHERE id={rejected}') == '0'
    assert 'CUSA-REJECT' not in sql('SELECT serial::text FROM releases WHERE id=1')
    request('/api/admin/serial-requests?status=garbage', status=400)
    archive = request('/api/admin/serial-requests?status=accepted&limit=1&offset=1')
    assert archive['total_count'] == 7 and len(archive['items']) == 1
    edited_id = submit(serial='USER-TYPO')['id']
    def accept_edit(serial, status=204, token=admin):
        return request(f'/api/admin/serial-requests/{edited_id}/accept', token, 'POST', json.dumps({'serial': serial}).encode(), status)
    accept_edit('', status=400)
    accept_edit('CUSA-FIXED', status=403, token=uploader)
    request(f'/api/admin/serial-requests/{edited_id}/archive', method='DELETE', status=409)
    accept_edit('  cusa-fixed ')
    assert 'USER-TYPO' not in sql('SELECT serial::text FROM releases WHERE id=1')
    assert 'CUSA-FIXED' in sql('SELECT serial::text FROM releases WHERE id=1')
    edited = next(row for row in request('/api/admin/serial-requests?status=accepted')['items'] if row['id'] == edited_id)
    assert edited['serial'] == 'CUSA-FIXED' and edited['submitted_serial'] == 'USER-TYPO'
    accept_edit('CUSA-FIXED')
    accept_edit('CUSA-DIFFERENT', status=409)
    assert 'CUSA-DIFFERENT' not in sql('SELECT serial::text FROM releases WHERE id=1')
    request(f'/api/admin/serial-requests/{edited_id}/archive', uploader, 'DELETE', status=403)
    request(f'/api/admin/serial-requests/{edited_id}/archive', method='DELETE', status=204)
    request(f'/api/admin/serial-requests/{edited_id}/photo', status=404)
    assert sql(f'SELECT count(*) FROM release_serial_requests WHERE id={edited_id}') == '0'
    assert 'CUSA-FIXED' in sql('SELECT serial::text FROM releases WHERE id=1')
    print('PASS: edited approval, original serial audit, conflicting retry, archive deletion/permissions and preserved catalogue serial')
    assert request('/api/kudos?login=victim', token=None)['kudos'] == 80
    collectors = request('/api/collectors')
    assert next(row for row in collectors if row['user_login'] == 'victim')['kudos'] == 80
    # Simulate an archive predating Kudos and replay the migration twice.
    sql(f'DELETE FROM kudos_awards WHERE request_id={entry_id}')
    assert request('/api/kudos?login=victim')['kudos'] == 70
    migration = Path(__file__).resolve().parents[1] / 'migrations/2026-09-10-160000-0000_kudos/up.sql'
    sql(migration.read_text()); sql(migration.read_text())
    assert request('/api/kudos?login=victim')['kudos'] == 80
    assert request('/api/kudos/challenge', token=None) == [{'user_login': 'victim', 'kudos': 80}]
    request('/api/kudos?login=missing', status=404)
    sql("INSERT INTO users(user_login,password_hash) SELECT 'pgr'||lpad(i::text,3,'0'),'fixture' FROM generate_series(1,105) i;")
    sql("INSERT INTO kudos_awards(request_id,user_id) SELECT -id,id FROM users WHERE user_login LIKE 'pgr%';")
    leaders = request('/api/kudos/challenge', token=None)
    assert len(leaders) == 100 and leaders[0]['user_login'] == 'victim'
    assert [row['user_login'] for row in leaders[1:]] == [f'pgr{i:03}' for i in range(1,100)]
    print('PASS: Kudos atomic/idempotent reward, archive deletion preservation, retroactive migration/replay, collectors and public top 100 tie ordering')
    for number in range(20): submit(serial=f'LIMIT-{number}')
    submit(serial='LIMIT-21', status=409)
    assert request('/api/admin/serial-requests')['total_count'] == 20
    print('PASS: serial requests: JPEG validation, console restrictions, existing serials/region, admin-only photos/review, transaction rollback, concurrent approval, archive, deletion and pending limit')

    from name_requests_contract import exercise as exercise_names
    exercise_names(request, sql, admin, uploader, jpeg)
