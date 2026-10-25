from pathlib import Path
import hashlib,json,re,subprocess
from PIL import Image
repo=Path('/home/sirati/devel/rust/civ3-renderer');root=Path('/mnt/datasets/c3r-build/jgl-t3-d94deaaf')
r=json.loads((root/'continuation-receipt.json').read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(root/r['source']['manifest'])==r['source']['sha256']
source_count=0
for line in (root/'source.sha256').read_text().splitlines():
 want,path=line.split(None,1)
 b=subprocess.run(['git','show',r['head']+':'+path.strip()],cwd=repo,check=True,capture_output=True).stdout
 assert hashlib.sha256(b).hexdigest()==want,path
 source_count+=1
assert source_count==515
for path,want in r['artifacts_sha256'].items():assert sha(root/path)==want,path
for path,record in r['tests'].items():assert sha(root/'tests'/path)==record['sha256'],path
pairs=[]
for b in r['runtime_buffer_equivalence']['buffers']:
 a=root/'runtime/diagnostic-default/buffers'/b['buffer'];p=root/'runtime/diagnostic-own/buffers'/b['buffer']
 assert a.read_bytes()==p.read_bytes() and a.stat().st_size==b['bytes'],b['buffer']
 pairs.append({'name':b['buffer'],'bytes':b['bytes'],'differences':0})
assert len(pairs)==8 and sum(p['bytes'] for p in pairs)==546720
images=[]
for result in r['final_screen_results']:
 a=Image.open(root/'runtime/diagnostic-default'/f"{result['screen']}.png").convert('RGB').crop(tuple(result['crop']))
 b=Image.open(root/'runtime/final-own'/f"{result['screen']}.png").convert('RGB').crop(tuple(result['crop']))
 assert a.size==b.size and a.tobytes()==b.tobytes(),result['screen']
 images.append(result)
base=json.loads((root/'baseline.json').read_text())
for mode in ['diagnostic-default','diagnostic-own','final-own']:
 actual={Path(path).name:want for want,path in [line.split(None,1) for line in (root/'runtime'/mode/'restoration.sha256').read_text().splitlines()]}
 assert all(actual[name]==value for name,value in base.items() if value is not None),mode
 assert not (root/'runtime'/mode/'quit-processes.txt').read_text().strip(),mode
for mode in ['diagnostic-own','final-own']:
 assert not re.search(r'Loaded .*jgl(?:\.dll)?["\s]',(root/'runtime'/mode/'wine.log').read_text(errors='replace'),re.I)
assert '40 calls, 1369360 bytes, zero differences' in (root/'tests/data-wine.log').read_text()
summary={'as_of':'2026-10-25','checkpoint':r['head'],'implementation_code':r['implementation_code'],'branch':r['branch'],'integration_accepted':False,'default_no_jgl_switch_accepted':False,'reviewed_behavior':'Caller-written navigator buffers alone retain equal-size, same-depth row copies and 16-bit clears through safe jgl::memory. Ordinary canvas draws stay records; no navigator bytes are sent to viewer.','independently_checked':{'source_hashes':source_count,'artifact_and_binding_hashes':len(r['artifacts_sha256']),'test_log_hashes':len(r['tests']),'buffer_pairs':pairs,'buffer_bytes':546720,'final_image_pairs':images,'restoration_and_empty_quit_receipts':['diagnostic-default','diagnostic-own','final-own'],'owned_loader_logs_without_jgl':['diagnostic-own','final-own'],'original_binary_test_log':'40 calls and 1,369,360 bytes with zero differences; parent reviewed log and test source, did not rerun.'},'limits':['Static PCGames HUD/advisor navigator buffer identities only; dynamic/replay navigators remain open.','Diagnostic F5 crop had 1,731 text-pixel differences; only the final production crop matches.','Requested map traces had zero draw records, so no trace-equivalence claim.','No physical Neo or complete group 6/group 7 acceptance.'],'remaining':r['remaining'],'contract_matrix':'../../jgl-contract-audit.md','evidence_root':str(root),'tests_reported':r['tests'],'known_failure':r['known_workspace_failure']}
path=Path('/mnt/datasets/c3r-build/jgl-data-review-20261025/review.json');path.write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({'source_hashes':source_count,'buffer_pairs':len(pairs),'buffer_bytes':546720,'final_image_pairs':len(images),'integration_accepted':False},indent=2))
