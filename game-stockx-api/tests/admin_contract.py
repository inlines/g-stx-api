#!/usr/bin/env python3
"""Exercise administrator access against disposable PostgreSQL/Redis containers.
Usage: python3 tests/admin_contract.py /absolute/path/to/game-stockx-api
Requires Docker, diesel and Node 22+ in PATH. Never uses the application database.
"""
import concurrent.futures
import base64
import hashlib
import hmac
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid
from http_contract import run, port, ROOT

binary = str(Path(sys.argv[1]).resolve())
suffix = uuid.uuid4().hex[:10]
pg, redis = 'gstx-admin-pg-' + suffix, 'gstx-admin-redis-' + suffix
created = []
process = None
try:
    for name, image, number, extra in [(pg, 'postgres:15', 5432, ['-e', 'POSTGRES_HOST_AUTH_METHOD=trust']), (redis, 'redis:alpine', 6379, [])]:
        run(['docker', 'run', '--rm', '-d', '--name', name, '-p', f'127.0.0.1::{number}', *extra, image])
        created.append(name)
    for _ in range(100):
        if subprocess.run(['docker', 'exec', pg, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres'], capture_output=True).returncode == 0:
            break
        time.sleep(.1)
    else:
        raise RuntimeError('Test PostgreSQL did not start')

    def sql(query):
        return run(['docker', 'exec', '-i', pg, 'psql', '-U', 'postgres', '-At', '-v', 'ON_ERROR_STOP=1'], input=query)

    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        http_port = probe.getsockname()[1]
    base = f'http://127.0.0.1:{http_port}'
    env = {**os.environ, 'DATABASE_URL': f'postgres://postgres@127.0.0.1:{port(pg,5432)}/postgres', 'REDIS_URL': f'redis://127.0.0.1:{port(redis,6379)}', 'BIND_ADDRESS': f'127.0.0.1:{http_port}', 'RUST_LOG': 'error'}
    with tempfile.TemporaryDirectory(prefix='gstx-admin-') as tmp:
        work = Path(tmp)
        run(['diesel', 'migration', 'run', '--config-file', '/dev/null', '--migration-dir', str(ROOT / 'migrations')], env=env, cwd=work)
        assert sql('SELECT count(*) FROM users WHERE is_admin') == '0'
        migration = ROOT / 'migrations/2026-09-10-120000-0000_administrators'
        sql((migration / 'down.sql').read_text())
        sql("INSERT INTO users(user_login,password_hash) VALUES('segasanshiro','fixture'),('ordinary','fixture');")
        sql((migration / 'up.sql').read_text())
        sql((migration / 'up.sql').read_text())
        assert sql("SELECT user_login FROM users WHERE is_admin") == 'segasanshiro'
        log = open(work / 'server.log', 'w')

        def start():
            global process
            process = subprocess.Popen([binary], cwd=work, env=env, stdout=log, stderr=log)
            for _ in range(100):
                try:
                    urllib.request.urlopen(base + '/health', timeout=.2).close()
                    return
                except Exception:
                    if process.poll() is not None:
                        raise RuntimeError((work / 'server.log').read_text())
                    time.sleep(.1)
            raise RuntimeError('API did not start')

        def request(path, token=None, data=None, method=None, status=200):
            time.sleep(.065)
            headers = {'Content-Type': 'application/json'}
            if token: headers['Authorization'] = 'Bearer ' + token
            req = urllib.request.Request(base + path, headers=headers, data=None if data is None else json.dumps(data).encode(), method=method)
            try: response = urllib.request.urlopen(req, timeout=15)
            except urllib.error.HTTPError as error: response = error
            raw = response.read().decode()
            assert response.status == status, (path, response.status, raw)
            try: return json.loads(raw)
            except ValueError: return raw

        def register(login):
            request('/api/register', data={'user_login': login, 'password': 'testpassword', 'is_admin': True}, status=201)

        def login(name):
            return request('/api/login', data={'user_login': name, 'password': 'testpassword'})['token']

        start()
        register('victim')
        sql("UPDATE users SET password_hash=(SELECT password_hash FROM users WHERE user_login='victim') WHERE user_login IN ('ordinary','segasanshiro');")
        admin, normal, victim = login('segasanshiro'), login('ordinary'), login('victim')
        assert request('/api/users/admin-badges') == ['segasanshiro']
        admin_id = request('/api/profile/me', admin)['id']
        ordinary_id = request('/api/profile/me', normal)['id']
        victim_id = request('/api/profile/me', victim)['id']
        assert request('/api/profile/me', victim)['is_admin'] is False
        for token, code in [(None, 401), ('invalid', 401), (normal, 403)]:
            request('/api/admin/users', token, status=code)
            request(f'/api/admin/users/{victim_id}/promote', token, data={}, status=code)
            request(f'/api/admin/users/{victim_id}', token, method='DELETE', status=code)
        # The former public signing key cannot forge an administrator.
        def b64(value): return base64.urlsafe_b64encode(json.dumps(value).encode()).rstrip(b'=')
        payload = b64({'alg': 'HS256', 'typ': 'JWT'}) + b'.' + b64({'sub': 'segasanshiro', 'uid': admin_id, 'exp': int(time.time()) + 3600})
        forged = (payload + b'.' + base64.urlsafe_b64encode(hmac.new(b'my-secret', payload, hashlib.sha256).digest()).rstrip(b'=')).decode()
        request('/api/admin/users', forged, status=401)
        page = request('/api/admin/users?query=ORD&limit=1', admin)
        assert page['total_count'] == 1 and page['items'][0]['user_login'] == 'ordinary'
        assert set(page['items'][0]) == {'id','user_login','is_admin','created_at'}
        assert request('/api/admin/users?query=%25', admin)['total_count'] == 0
        assert request('/api/admin/users?limit=1&offset=1', admin)['items'][0]['user_login'] == 'segasanshiro'
        request('/api/admin/users/2147483647/promote', admin, data={}, status=404)
        request('/api/admin/users/2147483647', admin, method='DELETE', status=404)
        request(f'/api/admin/users/{admin_id}', admin, method='DELETE', status=409)
        promoted = request(f'/api/admin/users/{ordinary_id}/promote', admin, data={})
        assert promoted['is_admin'] is True
        assert request('/api/users/admin-badges') == ['ordinary', 'segasanshiro']
        request(f'/api/admin/users/{ordinary_id}/promote', admin, data={}) # Idempotent
        request('/api/admin/users', normal) # No re-login needed for a newly promoted admin.
        # The signing key survives a backend restart; users remain logged in.
        process.terminate(); process.wait(timeout=10)
        start()
        request('/api/admin/users', admin)
        assert sql('SELECT count(*) FROM auth_signing_keys') == '1'
        sql("""INSERT INTO products(id,name,summary,first_release_date) VALUES(1,'Fixture','',1500000000);
            INSERT INTO platforms(id,name,abbreviation) VALUES(48,'PlayStation 4','PS4');
            INSERT INTO regions(id,name) VALUES(1,'Europe');
            INSERT INTO releases(id,product_id,platform,release_region) VALUES(1,1,48,1);
            INSERT INTO users_have_releases(release_id,user_login) VALUES(1,'victim'),(1,'ordinary');
            INSERT INTO users_have_wishes(release_id,user_login) VALUES(1,'victim');
            INSERT INTO users_have_wts(release_id,user_login,price,cib) VALUES(1,'victim',100,true),(1,'ordinary',200,false);
            UPDATE users SET avatar=decode('89504e47','hex') WHERE user_login='victim';
            INSERT INTO messages(sender_login,recipient_login,body) VALUES('victim','ordinary','out'),('ordinary','victim','in'),('ordinary','segasanshiro','keep');
        """)
        def restart_for_cache_test():
            process.terminate(); process.wait(timeout=10)
            start()
        from cache_contract import exercise as exercise_cache
        exercise_cache(lambda path, **kw: request(path, normal, **kw), sql, redis, restart_for_cache_test)
        from game_features_contract import exercise as exercise_features
        exercise_features(lambda path, **kw: request(path, normal, **kw), sql)
        from catalog_visibility_contract import exercise as exercise_visibility
        exercise_visibility(lambda path, **kw: request(path, normal, **kw), sql)
        from admin_direct_contract import exercise as exercise_direct
        exercise_direct(request, sql, admin, victim)
        from serial_requests_contract import exercise as exercise_serial_requests
        exercise_serial_requests(base, admin, victim, sql)
        notification_test = subprocess.run(['node', str(ROOT / 'tests/request_notifications.mjs')], input=json.dumps({'base': base, 'admin': admin, 'ordinary': victim}), text=True, capture_output=True)
        assert notification_test.returncode == 0, notification_test.stdout + notification_test.stderr
        print(notification_test.stdout.strip())
        # An unexpected dependent relation must roll the entire deletion back.
        sql(f'CREATE TABLE deletion_blocker(user_id INTEGER REFERENCES users(id)); INSERT INTO deletion_blocker VALUES({victim_id});')
        request(f'/api/admin/users/{victim_id}', admin, method='DELETE', status=500)
        assert sql("SELECT count(*) FROM messages WHERE sender_login='victim' OR recipient_login='victim'") == '2'
        assert sql("SELECT count(*) FROM users_have_wts WHERE user_login='victim'") == '1'
        sql('DROP TABLE deletion_blocker')
        print(run(['node', str(ROOT / 'tests/admin_ws.mjs')], input=json.dumps({'base': base, 'admin': admin, 'victim': victim, 'victimId': victim_id, 'observer': normal})))
        for table in ['users','users_have_releases','users_have_wishes','users_have_wts']:
            assert sql(f"SELECT count(*) FROM {table} WHERE user_login='victim'") == '0'
        assert sql("SELECT count(*) FROM messages") == '1'
        assert sql("SELECT count(*) FROM users_have_wts WHERE user_login='ordinary'") == '1'
        assert sql(f'SELECT count(*) FROM release_serial_requests WHERE submitter_id={victim_id}') == '0'
        assert sql(f'SELECT count(*) FROM kudos_awards WHERE user_id={victim_id}') == '0'
        request('/api/profile/me', victim, status=401)
        request('/api/avatars/victim', status=404)
        register('victim')
        request('/api/profile/me', victim, status=401)
        assert request('/api/profile/me', login('victim'))['id'] != victim_id
        # Deleting another admin is allowed; the remaining admin cannot delete self.
        request(f'/api/admin/users/{admin_id}', normal, method='DELETE', status=204)
        request('/api/admin/users', admin, status=401)
        assert request('/api/users/admin-badges') == ['ordinary']
        request(f'/api/admin/users/{ordinary_id}', normal, method='DELETE', status=409)
        register('segasanshiro')
        assert request('/api/profile/me', login('segasanshiro'))['is_admin'] is False
        # Check preflight for DELETE in cross-origin deployments.
        preflight = urllib.request.Request(base + f'/api/admin/users/{victim_id}', method='OPTIONS', headers={'Origin': 'http://localhost:4200', 'Access-Control-Request-Method': 'DELETE', 'Access-Control-Request-Headers': 'authorization'})
        with urllib.request.urlopen(preflight) as response:
            assert 'DELETE' in response.headers['Access-Control-Allow-Methods']
        # Two admins deleting each other concurrently must leave exactly one admin.
        replacement = login('segasanshiro')
        replacement_id = request('/api/profile/me', replacement)['id']
        request(f'/api/admin/users/{replacement_id}/promote', normal, data={})
        def competing_delete(token, target):
            req = urllib.request.Request(base + f'/api/admin/users/{target}', method='DELETE', headers={'Authorization': 'Bearer ' + token})
            try:
                with urllib.request.urlopen(req, timeout=15) as response: return response.status
            except urllib.error.HTTPError as error: return error.code
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
            first = executor.submit(competing_delete, normal, replacement_id)
            second = executor.submit(competing_delete, replacement, ordinary_id)
            assert sorted([first.result(), second.result()]) == [204, 401]
        assert sql('SELECT count(*) FROM users WHERE is_admin') == '1'
        print('PASS: bootstrap/absence/replay, role boundaries, forged JWT rejection, pagination/search, promotion, key persistence, transactional cascading deletion, account ID binding and DELETE CORS')
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired: process.kill(); process.wait()
    for name in reversed(created):
        subprocess.run(['docker','stop',name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
