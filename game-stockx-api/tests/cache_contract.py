"""Cache contract inside the disposable admin fixture; no production services."""
import subprocess
import time

def exercise(request, sql, redis, restart):
    def cli(*args):
        return subprocess.check_output(['docker','exec',redis,'redis-cli','--raw',*args],text=True).strip()
    def metric(cache, result):
        prefix=f'app_cache_reads_total{{cache="{cache}",result="{result}"}} '
        return float(next(line[len(prefix):] for line in request('/metrics').splitlines() if line.startswith(prefix)))
    cli('FLUSHDB')
    before=metric('product_basic','miss')
    assert request('/api/products/1')['product']['name']=='Fixture'
    assert metric('product_basic','miss')==before+1
    before=metric('product_basic','hit')
    assert request('/api/products/1')['product']['name']=='Fixture'
    assert metric('product_basic','hit')==before+1
    key=cli('KEYS','cache:v2:product_details:basic:1:*')
    assert 0<int(cli('TTL',key))<=86400
    cli('SET',key,'invalid json')
    before=metric('product_basic','error')
    assert request('/api/products/1')['product']['name']=='Fixture'
    assert metric('product_basic','error')==before+1
    # Changing the revision inside the import transaction makes retained keys obsolete.
    sql("BEGIN; UPDATE products SET name='Fresh fixture' WHERE id=1; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1; COMMIT;")
    assert request('/api/products/1')['product']['name']=='Fresh fixture'
    assert cli('EXISTS',key)=='1'
    sql("BEGIN; UPDATE products SET name='Fixture' WHERE id=1; UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1; COMMIT;")
    # Unresponsive Redis must neither prevent startup nor turn reads into 500s.
    subprocess.run(['docker','pause',redis],check=True,stdout=subprocess.DEVNULL)
    try:
        restart()
        started=time.monotonic()
        assert request('/api/products/1')['product']['name']=='Fixture'
        assert time.monotonic()-started<4
    finally:
        subprocess.run(['docker','unpause',redis],check=True,stdout=subprocess.DEVNULL)
    assert request('/api/products/1')['product']['name']=='Fixture'
    request('/api/products/2147483647',status=404)
    print('PASS: cache miss/hit/TTL, corrupt JSON fallback, transactional version invalidation, Redis timeout/startup/recovery, 404')
