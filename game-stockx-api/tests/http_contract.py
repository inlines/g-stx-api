#!/usr/bin/env python3
"""Compare two API binaries against isolated, disposable PostgreSQL and Redis containers.
Usage: python3 tests/http_contract.py /absolute/before-api /absolute/after-api
Requires Docker images postgres:15, redis:alpine and diesel CLI in PATH.
No existing databases or application containers are used.
"""
from pathlib import Path
import subprocess, json, os, time, urllib.request, urllib.error, socket
ROOT = Path(__file__).resolve().parents[1]

def run(args, **kw):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kw).stdout.strip()

def port(container, number):
    return run(['docker', 'port', container, str(number)]).rsplit(':', 1)[1]

def exercise(label, binary, pg, redis, W, franchises=False, companies=False):
    dbport = port(pg, 5432)
    redisport = port(redis, 6379)

    def sql(s):
        return run(['docker', 'exec', '-i', pg, 'psql', '-U', 'postgres', '-v', 'ON_ERROR_STOP=1'], input=s)
    sql('DROP SCHEMA public CASCADE; CREATE SCHEMA public;')
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        http_port = probe.getsockname()[1]
    env = {**os.environ, 'DATABASE_URL': f'postgres://postgres@127.0.0.1:{dbport}/postgres', 'REDIS_URL': f'redis://127.0.0.1:{redisport}', 'BIND_ADDRESS': f'127.0.0.1:{http_port}', 'RUST_LOG': 'error'}
    run(['diesel', 'migration', 'run', '--config-file', '/dev/null', '--migration-dir', str(ROOT / 'migrations')], env=env, cwd=W)
    sql("INSERT INTO covers VALUES(1,'cover');\n    INSERT INTO platforms(id,abbreviation,name,active,total_games) VALUES(48,'PS4','PlayStation 4',true,2);\n    INSERT INTO regions VALUES(1,'Europe');\n    INSERT INTO products(id,name,summary,cover_id,first_release_date) VALUES(1,'Alpha','Summary',1,1000),(2,'Beta','Summary',1,2000);\n    INSERT INTO releases(id,product_id,platform,release_region,serial,release_date) VALUES(1,1,48,1,ARRAY['ABC'],1000),(2,2,48,1,ARRAY['DEF'],2000);\n    INSERT INTO product_platforms(product_id,platform_id) VALUES(1,48),(2,48);\n    ")
    run(['docker', 'exec', redis, 'redis-cli', 'FLUSHALL'])
    log = open(W / (label + '-server.log'), 'w')
    process = subprocess.Popen([binary], cwd=W, env=env, stdout=log, stderr=log)
    results = []
    token = None

    def request(path, body=None, auth=True, header=None, record=True):
        time.sleep(0.08)
        headers = {}
        if body is not None:
            headers['Content-Type'] = 'application/json'
        if header is not None:
            headers['Authorization'] = header
        elif auth and token:
            headers['Authorization'] = 'Bearer ' + token
        req = urllib.request.Request(f'http://127.0.0.1:{http_port}' + path, data=None if body is None else json.dumps(body).encode(), headers=headers)
        try:
            r = urllib.request.urlopen(req, timeout=15)
        except urllib.error.HTTPError as e:
            r = e
        raw = r.read().decode()
        try:
            data = json.loads(raw)
        except ValueError:
            data = raw
        result = [path, body, r.status, r.headers.get('Content-Type'), data]
        if record:
            results.append(result)
        return result
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{http_port}/health', timeout=0.2).close()
                break
            except Exception:
                if process.poll() is not None:
                    raise RuntimeError('Server exited: ' + (W / (label + '-server.log')).read_text())
                time.sleep(0.1)
        request('/health')
        for name in ['alice', 'bob']:
            request('/api/register', {'user_login': name, 'password': 'testpassword'}, False)
        sql("INSERT INTO messages(sender_login,recipient_login,body,created_at) VALUES('alice','bob','Test message','2026-01-01 12:00:00+00');")
        request('/api/register', {'user_login': 'bad!', 'password': 'test'}, False)
        login = request('/api/login', {'user_login': 'alice', 'password': 'testpassword'}, False, record=False)
        assert login[2] == 200, login
        token = login[4]['token']
        if franchises:
            sql("INSERT INTO platforms(id,abbreviation,name,active,total_games) VALUES(167,'PS5','PlayStation 5',true,1); INSERT INTO product_platforms(product_id,platform_id) VALUES(1,167); INSERT INTO franschises VALUES(42,'Alpha series'),(43,'Shared series'),(44,'Empty series'); INSERT INTO game_franschises VALUES(42,1),(42,1),(43,1),(43,2);")
            metadata = request('/api/franchises/42', auth=False)
            assert metadata[2] == 200 and metadata[4] == {'id':42,'name':'Alpha series','platform_ids':[48,167]}, metadata
            assert request('/api/franchises/999', auth=False)[2] == 404
            assert request('/api/franchises/44', auth=False)[4]['platform_ids'] == []
            base = '/api/products?cat=48&limit=15&sort=name&ignore_digital=false'
            assert request(base)[4]['total_count'] == 2
            filtered = request(base+'&franchise_id=42')
            assert filtered[2] == 200 and filtered[4]['total_count'] == 1 and [x['id'] for x in filtered[4]['items']] == [1], filtered
            assert request(base+'&franchise_id=43')[4]['total_count'] == 2
            assert request(base+'&franchise_id=42')[4] == filtered[4]
            assert request(base)[4]['total_count'] == 2
            assert request(base+'&franchise_id=42&query=Beta')[4]['total_count'] == 0
            assert request(base+'&franchise_id=42&offset=1')[4]['items'] == []
            assert request('/api/products?cat=167&limit=15&franchise_id=42')[4]['total_count'] == 1
        if companies:
            sql("INSERT INTO platforms(id,abbreviation,name,active,total_games) VALUES(167,'PS5','PlayStation 5',true,1) ON CONFLICT DO NOTHING; INSERT INTO product_platforms(product_id,platform_id) VALUES(2,167); INSERT INTO companies(id,name) VALUES(42,'Studio'),(43,'Other studio'),(44,'Empty studio'); INSERT INTO products(id,name,summary) VALUES(3,'Port only',''); INSERT INTO product_platforms(product_id,platform_id) VALUES(3,48); INSERT INTO involved_companies(id,company,game,developer,publisher,porting) VALUES(1001,42,1,true,true,false),(1002,42,2,false,true,false),(1003,42,1,true,false,false),(1004,43,2,true,false,false),(1005,42,3,false,false,true);")
            metadata = request('/api/companies/42', auth=False)
            assert metadata[2] == 200 and metadata[4] == {'id':42,'name':'Studio','developer_platform_ids':([48,167] if franchises else [48]),'publisher_platform_ids':[48,167]}, metadata
            assert request('/api/companies/999', auth=False)[2] == 404
            assert request('/api/companies/44', auth=False)[4]['developer_platform_ids'] == []
            base = '/api/products?cat=48&limit=15&sort=name&ignore_digital=false'
            developer = request(base+'&company_id=42&company_role=developer')
            publisher = request(base+'&company_id=42&company_role=publisher')
            assert developer[2] == 200 and developer[4]['total_count'] == 1 and [x['id'] for x in developer[4]['items']] == [1], developer
            assert publisher[2] == 200 and publisher[4]['total_count'] == 2 and [x['id'] for x in publisher[4]['items']] == [1,2], publisher
            assert request(base+'&company_id=42&company_role=developer')[4] == developer[4]
            assert request(base+'&company_id=43&company_role=developer')[4]['items'][0]['id'] == 2
            assert request(base+'&company_id=42&company_role=developer&query=Beta')[4]['total_count'] == 0
            assert request(base+'&company_id=42&company_role=publisher&offset=1')[4]['items'][0]['id'] == 2
            assert request('/api/products?cat=167&limit=15&company_id=42&company_role=publisher')[4]['total_count'] == (2 if franchises else 1)
            assert request('/api/products?cat=167&limit=15&company_id=42&company_role=developer')[4]['total_count'] == (1 if franchises else 0)
            assert request(base+'&company_id=42&company_role=invalid')[2] == 400
        request('/api/login', {'user_login': 'alice', 'password': 'wrong'}, False)
        reads = ['/api/collection-stats', '/api/collection?cat=48', '/api/wishlist?cat=48', '/api/wts?cat=48', '/api/collectors', '/api/collectors/alice/wts', '/api/products/1', '/api/products/999', '/api/products?cat=48&limit=21', '/api/messages?companion=bob', '/api/dialogs']
        for path in reads:
            request(path, auth=False)
            request(path, header='Bearer broken')
        for path in ['/api/add_release', '/api/remove_release', '/api/set_release_price', '/api/add_wish', '/api/remove_wish', '/api/add_bid', '/api/remove_bid', '/api/add_wts', '/api/remove_wts']:
            request(path, {'release_id': 1}, auth=False)
        for path in ['/api/platforms', '/api/products?cat=48&limit=15&sort=name', '/api/products/1', '/api/products/999', '/api/collectors/alice/wts?limit=0']:
            result = request(path)
            assert result[2] < 500, result
        request('/api/platforms')
        request('/api/products/1')
        request('/api/add_wts', {'release_id': 1, 'price': 500, 'cib': True})
        request('/api/add_release', {'release_id': 1, 'product_id': 1})
        request('/api/set_release_price', {'release_id': 1, 'price': 0})
        request('/api/add_wts', {'release_id': 1, 'price': -1})
        request('/api/add_wts', {'release_id': 1, 'price': 500, 'cib': True})
        # The retired table can contain old flags on an upgraded database.
        migration = ROOT / 'migrations/2026-09-09-160000-0000_remove_bids'
        sql((migration / 'down.sql').read_text())
        sql("INSERT INTO users_have_bids VALUES (2, 'alice');")
        sql((migration / 'up.sql').read_text())
        sql((migration / 'up.sql').read_text())
        assert sql("SELECT to_regclass('users_have_bids') IS NULL;").splitlines()[2].strip() == 't'
        sale = request('/api/wts?cat=48')[4]['items'][0]
        assert sale['price'] == 500 and sale['cib'] is True, sale
        bob_token = request('/api/login', {'user_login': 'bob', 'password': 'testpassword'}, False, record=False)[4]['token']
        public = request('/api/products/1', header='Bearer ' + bob_token)
        assert public[2] == 200 and public[4]['releases'][0]['seller_logins'] == ['alice'], public
        own = request('/api/products/1')
        assert own[4]['releases'][0]['seller_logins'] == [], own
        stats = request('/api/collection-stats')
        assert stats[2] == 200 and 'bid_ids' not in stats[4][0], stats
        request('/api/add_wish', {'release_id': 2})
        assert request('/api/add_bid', {'release_id': 2})[2] == 404
        for path in reads:
            request(path)
        request('/api/collection-by-login/alice?cat=0&limit=1000&offset=0')
        request('/api/remove_wts', {'release_id': 1})
        assert request('/api/products/1', header='Bearer ' + bob_token)[4]['releases'][0]['seller_logins'] == []
        request('/api/collection-stats')
        request('/api/add_wts', {'release_id': 1, 'price': None, 'cib': False})
        request('/api/remove_release', {'release_id': 1})
        request('/api/wts?cat=48')
        request('/api/remove_wish', {'release_id': 2})
        assert request('/api/remove_bid', {'release_id': 2})[2] == 404
        request('/api/collection-stats')
        bob_ws_token = request('/api/login', {'user_login': 'bob', 'password': 'testpassword'}, False, record=False)[4]['token']
        print(run(['node', str(ROOT / 'tests/ws_auth.mjs')], input=json.dumps({'base': f'ws://127.0.0.1:{http_port}', 'alice': token, 'bob': bob_ws_token})))
        for _ in range(30):
            persisted = sql("SELECT sender_login FROM messages WHERE body = 'token-bound sender';")
            if 'alice' in persisted: break
            time.sleep(0.1)
        assert 'alice' in persisted and 'bob' not in persisted, persisted
        assert '0' in sql("SELECT COUNT(*) FROM messages WHERE body = 'blocked';")
        # Profile endpoints: wrong password must not log out the current token.
        password_path = '/api/profile/password'
        change = {'old_password': 'testpassword', 'new_password': 'new-password-123', 'confirm_password': 'new-password-123'}
        assert request(password_path, change, auth=False)[2] == 401
        assert request(password_path, {**change, 'confirm_password': 'different'})[2] == 400
        assert request(password_path, {**change, 'old_password': 'wrong'})[2] == 400
        assert request(password_path, change)[2] == 204
        assert request('/api/login', {'user_login': 'alice', 'password': 'testpassword'}, False, record=False)[2] == 401
        assert request('/api/login', {'user_login': 'alice', 'password': 'new-password-123'}, False, record=False)[2] == 200
        assert request(password_path, {'old_password': 'new-password-123', 'new_password': 'testpassword', 'confirm_password': 'testpassword'})[2] == 204
        import zlib, struct
        def png_bytes(w, h, color):
            def chunk(kind, data):
                return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
            return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', w, h, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress((b'\0' + bytes(color) * w) * h)) + chunk(b'IEND', b'')
        def avatar(path, data=None, auth=True):
            headers = {'Content-Type': 'image/png'}
            if auth: headers['Authorization'] = 'Bearer ' + token
            req = urllib.request.Request(f'http://127.0.0.1:{http_port}' + path, data=data, headers=headers)
            try: response = urllib.request.urlopen(req, timeout=15)
            except urllib.error.HTTPError as e: response = e
            return response.status, response.read(), response.headers
        assert avatar('/api/avatars/alice', auth=False)[0] == 404
        valid = png_bytes(64, 64, [10, 20, 30])
        assert avatar('/api/profile/avatar', valid, auth=False)[0] == 401
        assert avatar('/api/profile/avatar', b'invalid')[0] == 400
        assert avatar('/api/profile/avatar', png_bytes(64, 32, [10,20,30]))[0] == 400
        assert avatar('/api/profile/avatar', b'x' * 32769)[0] == 413
        assert avatar('/api/profile/avatar', valid)[0] == 204
        first = avatar('/api/avatars/alice', auth=False)
        assert first[0] == 200 and first[2]['Content-Type'] == 'image/png' and len(first[1]) < 32768
        assert struct.unpack('!II', first[1][16:24]) == (64, 64)
        assert avatar('/api/profile/avatar', b'invalid')[0] == 400
        assert avatar('/api/avatars/alice', auth=False)[1] == first[1]
        sql((ROOT / 'migrations/2026-09-09-170000-0000_user_avatar/up.sql').read_text())
        assert avatar('/api/avatars/alice', auth=False)[1] == first[1]
        assert avatar('/api/profile/avatar', png_bytes(64,64,[200,0,0]))[0] == 204
        assert avatar('/api/avatars/alice', auth=False)[1] != first[1]
        assert avatar('/api/avatars/bob', auth=False)[0] == 404
        print(label + ': profile password, avatar validation, replacement and migration checks passed')
        (W / (label + '-responses.json')).write_text(json.dumps(results, ensure_ascii=False, indent=2))
        print(label + ': ' + str(len(results)) + ' HTTP responses recorded')
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        log.close()
    return results
if __name__ == '__main__':
    import argparse, tempfile, uuid
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('--franchises', action='store_true', help='Also verify franchise metadata, filters, pagination and cache isolation')
    parser.add_argument('--companies', action='store_true', help='Verify company roles, metadata, pagination and cache isolation')
    args = parser.parse_args()
    suffix = uuid.uuid4().hex[:10]
    pg = 'gstx-contract-pg-' + suffix
    redis = 'gstx-contract-redis-' + suffix
    created = []
    try:
        for name, image, number, extra in [(pg, 'postgres:15', 5432, ['-e', 'POSTGRES_HOST_AUTH_METHOD=trust']), (redis, 'redis:alpine', 6379, [])]:
            run(['docker', 'run', '--rm', '-d', '--name', name, '-p', f'127.0.0.1::{number}', *extra, image])
            created.append(name)
        for attempt in range(100):
            ready = subprocess.run(['docker', 'exec', pg, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres'], capture_output=True)
            if ready.returncode == 0:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError('Test PostgreSQL did not start')
        with tempfile.TemporaryDirectory(prefix='gstx-contract-') as tmp:
            before = exercise('before', str(args.before.resolve()), pg, redis, Path(tmp), args.franchises, args.companies)
            after = exercise('after', str(args.after.resolve()), pg, redis, Path(tmp), args.franchises, args.companies)
            differences = [(a, b) for a, b in zip(before, after) if a != b]
            if len(before) != len(after) or differences:
                raise AssertionError(json.dumps(differences, ensure_ascii=False, indent=2))
            print(f'PASS: {len(before)} HTTP statuses, content types and response bodies match')
    finally:
        for name in reversed(created):
            subprocess.run(['docker', 'stop', name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
