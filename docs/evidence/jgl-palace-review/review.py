from pathlib import Path
import hashlib,json,re,subprocess
from PIL import Image
root=Path('/mnt/datasets/c3r-build/jgl-t3-2884caee/evidence')
images=[]
for name in ['palace-before','palace-hover','palace-after','palace-styles','palace-two-upgrades']:
 a=Image.open(root/'compare'/f'{name}.png').convert('RGB').crop((448,156,1472,924))
 for venue in ['own','final']:
  b=Image.open(root/venue/f'{name}.png').convert('RGB').crop((448,156,1472,924))
  assert a.size==b.size==(1024,768)
  assert a.tobytes()==b.tobytes(),(name,venue)
  images.append({'image':name,'compared_to':'compare','venue':venue,'crop':[448,156,1472,924],'different_pixels':0})
text=(root/'compare/bridge.log').read_text(errors='replace')
maps=[tuple(map(int,m)) for m in re.findall(r'Palace full-map compare: (\d+) cells, (\d+) differ, (\d+) unknown',text)]
assert len(maps)==7 and sum(m[0] for m in maps)==14515200 and not any(m[1] or m[2] for m in maps)
reads=[tuple(map(int,m)) for m in re.findall(r'Palace selection compare: query (\d+) original (\d+) model (\d+), (\d+) differ',text)]
assert len(reads)==15 and all(original==model and different==0 for _,original,model,different in reads)
owned=(root/'own/bridge.log').read_text(errors='replace')
assert len(re.findall(r'Palace state:.*jgl base None',owned))==15
for venue in ['own','final']:
 wine=(root/venue/'wine.log').read_text(errors='replace')
 assert not re.search(r'Loaded .*jgl(?:\.dll)?["\s]',wine,re.I),(venue,'jgl loader line')
artifacts=0
for line in (root/'production-artifacts.sha256').read_text().splitlines():
 expected,path=line.split(None,1);p=Path(path.strip())
 assert hashlib.sha256(p.read_bytes()).hexdigest()==expected,p
 artifacts+=1
sources=0
for line in (root/'production-source.sha256').read_text().splitlines():
 expected,path=line.split(None,1)
 b=subprocess.run(['git','show','b94eb47:'+path.strip()],check=True,capture_output=True,cwd='/home/sirati/devel/rust/civ3-renderer').stdout
 assert hashlib.sha256(b).hexdigest()==expected,path
 sources+=1
baseline={Path(path).name:digest for digest,path in [line.split(None,1) for line in (root/'baseline.sha256').read_text().splitlines()]}
for venue in ['compare','own','final','pass']:
 restored={Path(path).name:digest for digest,path in [line.split(None,1) for line in (root/venue/'restoration.sha256').read_text().splitlines()]}
 assert restored==baseline,venue
out={'as_of':'2026-10-25','branch':'t3/finish-jgl-groups-switch','checkpoint':'902ea9d0635d087f55c65da8dd207fc20a49db66','tested_source_commit':'b94eb477f77454a700564a25eaf0184432153fbb','integration_accepted':False,'default_replacement_enabled':False,'independently_checked':{'full_map_comparisons':len(maps),'full_map_cells':sum(m[0] for m in maps),'map_differences':0,'unavailable_map_answers':0,'actual_pixel_queries':len(reads),'query_differences':0,'owned_module_none_reports':15,'image_comparisons':images,'runtime_artifact_hashes':artifacts,'changed_source_hashes':sources,'restoration_receipts_match_baseline':['compare','own','final','pass'],'images_visually_reviewed':['own/rename.png','own/f1.png']},'functional_scope':'Papa Wine PCGames, one existing save, five Palace styles, two upgrades, label behavior and confirmed Quit. No Neo result or full group acceptance follows.','review_objections':['Synchronous file read and PCX decode occur on the game-thread Palace blit path through ui_opacity::require_file.','Pointer anchor rebuilding scans piece rectangles on the game thread.','Object/DC/read-back/registry/font contracts and broader window/pump/input equivalence remain incomplete.'],'next_action':'A new GPT-6.1 Sol continuation owns corrections and remaining groups in an isolated worktree; keep this branch unmerged.','retained_evidence_root':str(root)}
path=Path('/mnt/datasets/c3r-build/jgl-parent-review-20261025/review.json');path.write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps({'maps':len(maps),'cells':sum(m[0] for m in maps),'reads':len(reads),'image_comparisons':len(images),'source_hashes':sources,'artifact_hashes':artifacts,'integration_accepted':False},indent=2))
