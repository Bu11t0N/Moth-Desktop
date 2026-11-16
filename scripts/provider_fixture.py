"""Synthetic provider endpoints for disposable native HTTPS acceptance only."""
import json

DEVICE = '00000000-0000-4000-8000-000000000009'
BODY = b'trojan://synthetic@example.com:443#One\ntrojan://synthetic@other.example.com:443#Two\n'

def handle_provider(request, origin):
    path=request.path
    if not path.startswith('/provider/'):
        return False
    case=path.removeprefix('/provider/')
    expected=case in ('device','redirect','cross')
    actual=request.headers.get('x-hwid')
    valid=(actual==DEVICE if expected else actual is None)
    valid=valid and request.headers.get('User-Agent')=='Moth/fixture'
    if expected:
        valid=valid and request.headers.get('x-device-os') in ('Windows','Linux') and request.headers.get('x-device-model','').startswith('Desktop ') and bool(request.headers.get('x-ver-os'))
    else:
        valid=valid and all(request.headers.get(k) is None for k in ('x-device-os','x-device-model','x-ver-os'))
    if not valid:
        request.send_response(400);request.send_header('Content-Length','0');request.end_headers();return True
    if case in ('redirect','cross','downgrade'):
        target={'redirect':origin+'/provider/device','cross':origin.replace('localhost','127.0.0.1')+'/provider/cross-target','downgrade':origin.replace('https:','http:')+'/provider/opt-out'}[case]
        request.send_response(302);request.send_header('Location',target);request.send_header('Content-Length','0');request.end_headers();return True
    if case=='denied':
        request.send_response(403);request.send_header('Content-Length','0');request.end_headers();return True
    request.send_response(200)
    if case=='limit':request.send_header('x-hwid-limit','true')
    request.send_header('subscription-userinfo','upload=10; download=20; total=100; expire=2000000000')
    body=BODY
    if case=='partial':body=BODY+b'unsupported://synthetic\n'
    if case=='json':body=json.dumps({'outbounds':[{'type':'trojan','tag':'JSON','server':'example.com','server_port':443,'password':'synthetic','tls':{'enabled':True}}]}).encode()
    if case=='yaml':body=b'proxies:\n  - name: YAML\n    type: trojan\n    server: example.com\n    port: 443\n    password: synthetic\n'
    request.send_header('Content-Length',str(2*1024*1024+1 if case=='oversize' else len(body)))
    request.end_headers()
    if case!='oversize':request.wfile.write(body)
    return True
