"""Actual binary + Rust ledger, isolated TEST roots; no provider charge/browser."""
from terminal_process import wait_output
import ast, base64, fcntl, json, os, re, select, struct, sys, tempfile, termios, time
import test_launch as subprocess
from pathlib import Path
from vt_screen import Screen
REPO=Path(__file__).resolve().parents[1]
BINARY=REPO/'target/debug/tinkery'
HOST='''#!/usr/bin/env python3
import json,sys
from pathlib import Path
with Path('provider-calls').open('a') as log: log.write('call\\n')
raw=sys.stdin.read()
with Path('provider-requests').open('a',encoding='utf-8') as log: log.write(' '.join(sys.argv)+raw+'\\n')
r=json.loads(raw)
if r.get('kind')=='meaning-preservation': print(json.dumps(dict(missing=[],false_choice=False)));sys.exit(0)
if r.get('kind')=='question-continuity':print('{"repeated":[]}');sys.exit(0)
print(json.dumps(dict(parts=dict(who='I',outcome='judge a PR ready for human review as restaurateur while both the harness and codebase compound',why='I retain final judgment',done_when='a PR is ready for human review',must=['human taste'],must_not=['cook','show cost unless it is way off','build a dashboard']),open=['why'],supports=[dict(source=s['id'],quote=s['text'],occurrence=0) for s in r['sources']],misfits=[],questions=[dict(target='why',id='open',text='Which UI or consequential design checkpoints remain unresolved?')],alternatives=[])))
'''
import session_fixture as fixture

def helper(*args):
 if args[0]=='init':return {'session':str(fixture.create(args[2]))}
 return getattr(fixture, {'read':'load'}.get(args[0],args[0]))(*args[1:])
def journey(width,height,home=False,menu_only=False,home_case='absolute',session_case='explicit',in_repo=False,literals=False):
 with tempfile.TemporaryDirectory(prefix='tinkery-TEST-goal-') as directory:
  cwd=Path(directory);root=cwd/'TEST-sessions' if session_case!='default' else cwd/'TEST-home/.local/share/tinkery/sessions'
  records=cwd/'TEST-records';host=cwd/'pi';host.write_text(HOST.replace("must=['human taste']", "must=['human taste', 'accept \"A  B\"', 'accept \"A B\"', 'must Accept \"A B\"', 'use IT', 'use it']") if literals else HOST);host.chmod(0o755)
  # Inside a repository, start from a nested folder: Tinkery names the root; stores stay outside it.
  repo=cwd/'TEST-repo';run=repo/'nested' if in_repo else cwd
  if in_repo:(repo/'.git').mkdir(parents=True);run.mkdir()
  root_args=[] if session_case=='default' else ['--seed-session-root',str(root) if session_case=='explicit' else 'TEST-sessions']
  # Observe even short-lived helper/provider Python launches without changing the official helper.
  python=cwd/'python3'
  python.write_text(f'#!{sys.executable}\nimport os,sys\nwith open({str(cwd/"python-launches")!r}, "a") as log: log.write(repr(sys.argv[1:])+"\\n")\nos.execv({sys.executable!r}, [{sys.executable!r}, *sys.argv[1:]])\n')
  python.chmod(0o755)
  master,slave=os.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',height,width,0,0));screen=Screen(width,height)
  environment={**os.environ,'TERM':'xterm-256color','PATH':str(cwd)+os.pathsep+os.environ['PATH'],'HOME':str(cwd/'TEST-home')}
  if home_case=='unset':environment.pop('HOME')
  elif home_case=='empty':environment['HOME']=''
  elif home_case=='relative':environment['HOME']='TEST-relative-home'
  process=subprocess.Popen([str(BINARY),*(['--home'] if home else []),'--shape-pi', '--shape-budget', '64','--model','test/model',*root_args,'--record-root',str(records),'--link-root',str(cwd/'TEST-links'),'--pi-command',str(host)],stdin=slave,stdout=slave,stderr=slave,cwd=run,env=subprocess.environment(environment, home=(environment)['HOME']))
  raw=bytearray()
  def feed():
   data=os.read(master,65536);raw.extend(data);screen.feed(data)
  def right():
   return ' '.join(' '.join(line[width*48//100+2:].strip() for line in screen.text().splitlines()).split())
  def descendants():
   rows=subprocess.run(['ps','-axo','pid=,ppid=,command='],capture_output=True,text=True,check=True, env=subprocess.environment()).stdout.splitlines()
   entries=[line.strip().split(None,2) for line in rows]
   found={process.pid};children=set()
   while True:
    added={int(pid) for pid,ppid,*_ in entries if int(ppid) in found}-found
    if not added: return children
    found.update(added);children.update(added)
  def await_text(text,timeout=30):
   deadline=time.monotonic()+timeout
   def consume_wait_output(chunk):
       raw.extend(chunk)
       screen.feed(chunk)
   for returncode in wait_output(master, process, consume_wait_output, deadline):
    if text in screen.text() and not screen.synchronized:
     assert screen.cells[1][2:9]==list('tinkery') and screen.cells[1][width-3]=='?',screen.text()
     return
    if returncode is not None:break
   raise AssertionError(f'Missing {text!r}:\n{screen.text()}')
  def absent(text):
   deadline=time.monotonic()+5
   def consume_wait_output(chunk):
       raw.extend(chunk)
       screen.feed(chunk)
   for _ in wait_output(master, process, consume_wait_output, deadline):
    if text not in screen.text():return
   raise AssertionError(f'Stale modal still visible: {text}')
  def send(data,text):os.write(master,data);await_text(text)
  def review():send(b'\x1bOP','Review the goal');send(screen.pick('Review the goal'),'You are confirming')
  try:
   if home:
    await_text('think');title=screen.text().splitlines()[1]
    assert ('tinkery  in TEST-repo' in title)==in_repo and (' in ' in title)==in_repo,title
    send(b'1',"What's on your mind?")
   else: await_text("What's on your mind?")
   assert not root.exists() and not records.exists()
   dump='  PR ready for human review. restaurateur, not cook. Café 👩‍💻; harness and codebase both compound. Cost only when it\'s way off; no dashboard.  '
   if literals:dump += ' Accept "A  B" and "A B"; use IT and use it.'
   os.write(master,b'\x1b[200~'+dump.encode()+b'\x1b[201~');await_text('compound.')
   send(b'\x1bOQ','Which UI');assert not root.exists() and not records.exists()
   # Unsent words hide Review. Help navigation neither sends nor changes that draft.
   before_calls=(run/'provider-calls').read_bytes()
   send(b'x\x1bOP','Right now');assert '1  Review the goal' not in screen.text(),screen.text()
   os.write(master,b'\x1bOQ');time.sleep(.1)
   assert (run/'provider-calls').read_bytes()==before_calls and not root.exists() and not records.exists()
   send(b'\x1b','F1 more');assert (run/'provider-calls').read_bytes()==before_calls;os.write(master,b'\x7f')
   if literals:
    await_text('Must: accept "A  B".')
    assert 'Must: accept "A B".' in screen.text(),screen.text()
   review();assert ('made in TEST-repo' in right())==in_repo and ('made in' in right())==in_repo,screen.text();assert "type confirm to save the goal and its record. You can't edit it after." in right() or "PgDn: read to the end first." in right(),screen.text();assert not root.exists() and not records.exists()
   if literals:
    assert 'Must: accept "A  B".' in screen.text() and 'Must: accept "A B".' in screen.text(),screen.text()
   # A refused Enter says why where the confirm prompt was: scroll first, or type the word.
   send(b'\r','Enter.');shown=' '.join(right().split());assert 'Read to the end first: PgDn scrolls. Then type confirm and Enter.' in shown or 'Type confirm, then Enter. Nothing saved.' in shown,screen.text()
   os.write(master,b'\x1bOQ');time.sleep(.1);assert not root.exists() and not records.exists(),'Opening/Enter/F2 implied consent'
   os.write(master,b'\x1b');absent('You are confirming');assert not root.exists() and not records.exists()
   review()
   os.write(master,b'\x1b[6~'*30);await_text('remain unresolved?')
   send(b'confirm\r','Saved.');await_text('Next:')
   assert 'Next: shape it into a seed in Shape.' in right(),screen.text()
   assert screen.text().splitlines()[-1].strip()=='s open in tend   esc menu'
   assert str(root) not in screen.text() and 'seed not written yet' not in screen.text()
   first_goal=right()
   sessions=list(root.iterdir());assert len(sessions)==1
   session=sessions[0].resolve();ledger=helper('read',session);status=helper('status',session)
   assert ledger['status']=='active' and ledger['operator']=='human'
   assert not any(k in status for k in ('viewer','snapshot','snapshot_current'))
   origin=next(n for n in ledger['nodes'] if n['id']==ledger['origin'])
   assert origin['status']=='settled' and origin['kind']=='decision' and origin['authority']=='user'
   assert origin['answer']==ledger['goal']=='I judge a PR ready for human review as restaurateur while both the harness and codebase compound.\nWhy: I retain final judgment.\nDone when: a PR is ready for human review.\nMust: human taste.'+('\nMust: accept \"A  B\".\nMust: accept \"A B\".\nMust: use IT.\nMust: use it.' if literals else '')+'\nMust not: cook.\nMust not: show cost unless it is way off.\nMust not: build a dashboard.'
   context=json.loads(origin['evidence'][-1]['observed'])
   assert ledger['draft']['outcome']==context['success']==context['goal_parts']['done_when']=='a PR is ready for human review'
   assert context['constraints']==dict(must=(['human taste', 'accept \"A  B\"', 'accept \"A B\"', 'use IT', 'use it'] if literals else ['human taste']),must_not=['cook','show cost unless it is way off','build a dashboard'])
   assert context['problem']==context['goal_parts']['why']=='I retain final judgment'
   assert any(e.get('observed')==dump and e.get('checked')=='Tinkery original 1' for e in origin['evidence']);assert 'unresolved' in origin['evidence'][-1]['observed']
   assert not (session/'seed-contract.md').exists()
   # The same typed confirm wrote exactly one think record: the words, the turns in order, the goal as saved.
   written=list(records.iterdir());assert len(written)==1,written
   record=json.loads(written[0].read_text());record_bytes=written[0].read_bytes()
   assert written[0].name.endswith('_'+session.name+'.json'),written[0].name
   assert record['schema']==3 and record['origin']=='live' and record['model']=='test/model' and record['seed_me_session']==session.name
   assert Path(record['seed_me_session_path']) == session
   assert (Path(record['repo']).resolve()==repo.resolve()) if in_repo else record['repo'] is None,record['repo']
   assert 'TEST-repo' not in (run/'provider-requests').read_text(),'the repository reached the provider'
   assert 'TEST-repo' not in (session/'ledger.json').read_text(),'the repository reached the session ledger'
   assert record['goal']==ledger['goal'],(record['goal'],ledger['goal'])
   assert [t['kind'] for t in record['turns']]==['words','asked'],record['turns']
   assert record['turns'][0]['text']==dump and record['turns'][1]['id']=='open'
   assert record['checks'] and all(c['decision']=='pass' for c in record['checks'])
   assert all(c['raw'] is None for c in record['checks']),'a passing check kept the model reply'
   failed=subprocess.run([str(fixture.BINARY)],input=json.dumps(dict(op='end',path=str(session),status='completed',reason='TEST must refuse incomplete seed')),capture_output=True,text=True, env=subprocess.environment())
   assert failed.returncode!=0 and 'seed-contract.md' in failed.stderr
   assert helper('read',session)['status']=='active'
   saved=(session/'ledger.json').read_bytes()
   calls=(run/'provider-calls').read_bytes()
   launches=(cwd/'python-launches').read_bytes() if (cwd/'python-launches').exists() else b''
   assert not any('init' in args for args in map(ast.literal_eval,launches.decode().splitlines())), 'Saving invoked an external session helper'
   deadline=time.monotonic()+3
   owned=descendants()
   while owned and time.monotonic()<deadline:
    time.sleep(.02)
    owned=descendants()
   assert not owned, subprocess.check_output(['ps','-p',','.join(map(str,owned)),'-o','pid=,command='],text=True, env=subprocess.environment()) if owned else owned
   os.write(master,b'\x07confirm\r\x1bOQ\x1bOR\x1bOS\x1b[15~\x1b[17~\x1b[19~\x1b[20~\x0e\x0f\x04q')
   time.sleep(.2)
   os.write(master,b'c');time.sleep(.1)
   assert not re.findall(rb'\x1b\]52;c;',bytes(raw)), 'Retired continuation prompt was copied'
   assert descendants()==owned and (run/'provider-calls').read_bytes()==calls
   assert ((cwd/'python-launches').read_bytes() if (cwd/'python-launches').exists() else b'')==launches
   assert (session/'ledger.json').read_bytes()==saved and len(list(root.iterdir()))==1
   assert list(records.iterdir())==written and written[0].read_bytes()==record_bytes
   os.write(master,b'\x1b[6~'*30);await_text('Must not: build a dashboard.')
   assert 'Next: shape it into a seed in Shape.' in right(),screen.text()
   for line in ledger['goal'].splitlines(): assert ' '.join(line.split()) in first_goal or ' '.join(line.split()) in right(),(line,first_goal,screen.text())
   send(b'\x1bOP','Right now');send(b'h','┌ How it works ')
   # Page down one step at a time: the record path follows Seed Me's, and its length varies by platform.
   seen=[right()]
   for _ in range(16):
    if 'Think record:' in seen[-1]:break
    previous=right();before=screen.frames
    os.write(master,b'\x1b[6~');deadline=time.monotonic()+5
    def consume_wait_output(chunk):
        raw.extend(chunk)
        screen.feed(chunk)
    for _ in wait_output(master, process, consume_wait_output, deadline):
     if screen.ready(before) and right()!=previous and '┌ How it works ' in screen.text():break
    else:raise AssertionError(f'Help page did not advance before Think record:\n{screen.text()}')
    seen.append(right())
   assert 'Think record:' in seen[-1],screen.text()
   assert any('Session:' in page for page in seen),seen
   os.write(master,b'\x1b');await_text('Right now')
   assert 'send' not in right() and 'speak' not in right()
   os.write(master,b'\x1b');await_text('Saved.')
   assert str(root) not in screen.text()
   if menu_only:
    send(b'\x1b','think')
    assert process.poll() is None and 'tend' in screen.text()
   else:
    # Make the target non-first with the official helper, not hand-written canonical files.
    helper('init','--root',root)
    send(b's','s shape   r history   esc back   F10 menu')
    assert 'tinkery › tend' in screen.text()
    assert 'Saved.' not in screen.text() and 'any harness' not in screen.text()
    assert str(root) not in screen.text() and session.name not in screen.text()
    tend_block=' '.join(screen.text().split())
    for line in ledger['goal'].splitlines(): assert ' '.join(line.split()) in tend_block,(line,screen.text())
    os.write(master,b'c');time.sleep(.1)
    assert not re.findall(rb'\x1b\]52;c;',bytes(raw))
    send(b'\x1b','↑↓←→ move   enter open   l link   esc back')
    assert 'Why:' not in screen.text() and 'Done when:' not in screen.text()
    send(b'\r','s shape   r history   esc back   F10 menu')
    assert 'Must not: build a dashboard.' in screen.text()
    send(b'\x1b[21~','think')
   assert (session/'ledger.json').read_bytes()==saved
   assert list(records.iterdir())==written and written[0].read_bytes()==record_bytes
   assert not (session/'viewer.json').exists()
   assert not descendants() and (run/'provider-calls').read_bytes()==calls
   assert ((cwd/'python-launches').read_bytes() if (cwd/'python-launches').exists() else b'')==launches
   os.write(master,b'\x1b[21~');deadline=time.monotonic()+10
   def consume_exit_output(chunk):
    raw.extend(chunk)
    screen.feed(chunk)
   for returncode in wait_output(master,process,consume_exit_output,deadline):
    if returncode is not None:break
   assert process.poll()==0,screen.text()
   assert ('Think record: '+str(written[0])).encode() in bytes(raw),bytes(raw)[-2000:]
   assert not (session/'viewer.json').exists()
   assert helper('read',session)['status']=='active';assert not (session/'seed-contract.md').exists()
   print(json.dumps(dict(width=width,height=height,provider='stub',ledger='Rust',think_record='one, verified',test_confirmation='AUTOMATED TEST, NOT OPERATOR',goal_only=True,receipt_keys=['s','Esc'],entry='home' if home else 'standalone',navigation='Esc' if menu_only else 's',viewer_started=False,session_root='isolated temporary directory',home_case=home_case,session_case=session_case,in_repo=in_repo)))
  finally:
   if process.poll() is None:process.kill();process.wait()
   os.close(master);os.close(slave)
for home in [False,True]:journey(80,24,home=home,session_case='default')
# Unsafe HOME launches are refused by test_launch; its mock-only suite covers them.
for home in [False,True]:
 for size in [(80,24),(100,30),(160,40),(320,40)]:journey(*size,home=home)
 journey(80,24,home=home,menu_only=True)
 for size in [(80,24),(100,30)]:journey(*size,home=home,in_repo=True)

# Literal spacing survives public input, review, typed confirmation and session saving.
journey(100,30,literals=True)
