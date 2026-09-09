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

def exercise(label, binary, pg, redis, W):
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
    sql("INSERT INTO covers VALUES(1,'cover');\n    INSERT INTO platforms(id,abbreviation,name,active,total_games) VALUES(48,'PS4','PlayStation 4',true,2);\n    INSERT INTO regions VALUES(1,'Europe');\n    INSERT INTO products(id,name,summary,cover_id) VALUES(1,'Alpha','Summary',1),(2,'Beta','Summary',1);\n    INSERT INTO releases(id,product_id,platform,release_region,serial,release_date) VALUES(1,1,48,1,ARRAY['ABC'],1000),(2,2,48,1,ARRAY['DEF'],2000);\n    INSERT INTO product_platforms(product_id,platform_id) VALUES(1,48),(2,48);\n    ")
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
        request('/api/add_wish', {'release_id': 2})
        request('/api/add_bid', {'release_id': 2})
        for path in reads:
            request(path)
        request('/api/collection-by-login/alice?cat=0&limit=1000&offset=0')
        request('/api/remove_wts', {'release_id': 1})
        request('/api/collection-stats')
        request('/api/add_wts', {'release_id': 1, 'price': None, 'cib': False})
        request('/api/remove_release', {'release_id': 1})
        request('/api/wts?cat=48')
        request('/api/remove_wish', {'release_id': 2})
        request('/api/remove_bid', {'release_id': 2})
        request('/api/collection-stats')
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
            before = exercise('before', str(args.before.resolve()), pg, redis, Path(tmp))
            after = exercise('after', str(args.after.resolve()), pg, redis, Path(tmp))
            differences = [(a, b) for a, b in zip(before, after) if a != b]
            if len(before) != len(after) or differences:
                raise AssertionError(json.dumps(differences, ensure_ascii=False, indent=2))
            print(f'PASS: {len(before)} HTTP statuses, content types and response bodies match')
    finally:
        for name in reversed(created):
            subprocess.run(['docker', 'stop', name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
