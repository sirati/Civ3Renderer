from pathlib import Path
import hashlib,json,re,subprocess
from PIL import Image
repo=Path('/home/sirati/devel/rust/civ3-renderer')
root=Path('/mnt/datasets/c3r-build/jgl-t3-e967e7bc')
r=json.loads((root/'continuation-receipt.json').read_text())
for path,want in r['source_sha256'].items():
 b=subprocess.run(['git','show',r['commit']+':'+path],cwd=repo,capture_output=True,check=True).stdout
 assert hashlib.sha256(b).hexdigest()==want,path
for path,want in r['backup_sha256'].items():assert hashlib.sha256((root/path).read_bytes()).hexdigest()==want,path
for path,want in r['runtime_artifacts_sha256'].items():assert hashlib.sha256((root/path).read_bytes()).hexdigest()==want,path
for path,want in r['evidence_sha256'].items():assert hashlib.sha256((root/path).read_bytes()).hexdigest()==want,path
log=(root/'evidence/diagnostic/bridge.log').read_text(errors='replace')
comparisons=[tuple(map(int,m)) for m in re.findall(r'source compare: .*cells=(\d+), differences=(\d+), palette_differences=(\d+)',log)]
assert len(comparisons)==203 and sum(c[0] for c in comparisons)==23713540 and all(c[1:]==(0,0) for c in comparisons)
images=[]
old=Path('/mnt/datasets/c3r-build/jgl-t3-2884caee/evidence/compare')
for name in ['palace-before','palace-after','palace-styles','palace-two-upgrades']:
 a=Image.open(old/f'{name}.png').convert('RGB').crop((448,156,1472,924))
 b=Image.open(root/'evidence/final'/f'{name}.png').convert('RGB').crop((448,156,1472,924))
 assert a.size==b.size==(1024,768) and a.tobytes()==b.tobytes(),name
 images.append({'name':name,'crop':[448,156,1472,924],'different_pixels':0})
final=root/'evidence/final'
artifacts=0
for line in (final/'artifacts.sha256').read_text().splitlines():
 want,path=line.split(None,1);assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==want,path
 artifacts+=1
restores=[]
expected={'IFC23.dll':'079641fbec833579233df78e7a5426b38e559b4948c7e6f3bc0ebc123cb65b0e','civ3r_native.dll':'4151b134ac7c2076abea13644e54519a95c8bb349cafdf3c01c1e85d35ead935','conquests.ini':'327cd772872b1f7e82123b6f80b52d5505b0c23aa172482f59f71c9c587a5fd6'}
for relative in r['restoration_receipts']:
 actual={Path(path).name:digest for digest,path in [l.split(None,1) for l in (root/relative).read_text().splitlines()]}
 assert actual==expected,relative
 restores.append(relative)
assert (final/'quit-processes.txt').read_text().strip()==''
assert not re.search(r'Loaded .*jgl(?:\.dll)?["\s]',(final/'wine.log').read_text(errors='replace'),re.I)
summary={'as_of':'2026-10-25','checkpoint':r['commit'],'branch':r['branch'],'integration_accepted':False,'default_no_jgl_switch_accepted':False,'reviewed_corrections':{'hot_hook_file_IO_and_decode':'removed; retains already decoded source at load','game_thread_anchor_scan':'moved to publisher using the same captured Present and immutable selection revision','source_capture_cost':'One guarded block copy at file load; scales with source size. No constant-time or zero-copy claim.','registry_and_font':'Safe circular head/cursor state, embedded-base deletion and owned FOT storage reviewed; original-jgl/GDI test logs retained.'},'independently_checked':{'source_hashes':len(r['source_sha256']),'source_backup_hashes':len(r['backup_sha256']),'source_load_comparisons':len(comparisons),'source_indices':sum(c[0] for c in comparisons),'index_differences':0,'palette_differences':0,'image_comparisons':images,'final_artifact_hashes':artifacts,'restoration_receipts':restores,'final_quit_process_receipt_empty':True,'final_loader_jgl_load_lines':0},'remaining':r['remaining'],'tests_reported':r['tests'],'evidence_root':str(root),'scope':'Papa Wine PCGames only; neither complete group 6/group 7 acceptance nor a Neo runtime result. Parent inspected source/evidence and did not rerun the suites.'}
out=Path('/mnt/datasets/c3r-build/jgl-source-review-20261025/review.json');out.write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({'source_hashes':len(r['source_sha256']),'loads':len(comparisons),'indices':sum(c[0] for c in comparisons),'final_image_pairs':len(images),'restoration_receipts':len(restores),'integration_accepted':False},indent=2))
