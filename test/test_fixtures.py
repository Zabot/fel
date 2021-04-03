def test_origin(upstream, origin):
    assert upstream != origin
    assert upstream.head.commit == origin.head.commit

def test_upstream(upstream):
    commits = [c.summary for c in upstream.iter_commits()]
    assert commits == list(reversed(list(map(str, range(4)))))
