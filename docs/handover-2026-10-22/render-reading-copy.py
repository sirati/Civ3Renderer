#!/usr/bin/env python3
"""Rebuild the offline handover from its committed Markdown and JSON records."""

import html
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def inline(value):
    value = html.escape(value)
    value = re.sub(r'`([^`]+)`', r'<code>\1</code>', value)
    value = re.sub(r'\*\*([^*]+)\*\*', r'<strong>\1</strong>', value)

    def link(match):
        label, href = match.groups()
        pages = {'README.md': '#overview', 'subagents.md': '#agents',
                 'acceptance.md': '#acceptance', 'verification.md': '#verification',
                 'backup-and-restore.md': '#backups', 'deferred-work.md': '#deferred',
                 'handover.html': '#overview'}
        href = pages.get(href, href)
        return f'<a href="{href}">{label}</a>'

    return re.sub(r'\[([^\]]+)\]\(([^)]+)\)', link, value)


def markdown(text):
    """Render the small Markdown subset used by the overview documents."""
    out, paragraph, table, listing, code = [], [], False, None, None

    def flush():
        if paragraph:
            out.append('<p>' + inline(' '.join(paragraph)) + '</p>')
            paragraph.clear()

    def close_list():
        nonlocal listing
        if listing:
            out.append('</' + listing + '>')
            listing = None

    for line in text.splitlines():
        if code is not None:
            if line.startswith('```'):
                out.append('<pre><code>' + html.escape('\n'.join(code)) + '</code></pre>')
                code = None
            else:
                code.append(line)
            continue
        if line.startswith('```'):
            flush()
            close_list()
            code = []
            continue
        if line.startswith('|'):
            flush()
            close_list()
            cols = [x.strip() for x in line.strip().strip('|').split('|')]
            if all(re.fullmatch(r':?-+:?', x) for x in cols):
                continue
            if not table:
                out.append('<div class="table-scroll"><table><thead><tr>' + ''.join('<th>' + inline(x) + '</th>' for x in cols) + '</tr></thead><tbody>')
                table = True
            else:
                out.append('<tr>' + ''.join('<td>' + inline(x) + '</td>' for x in cols) + '</tr>')
            continue
        if table:
            out.append('</tbody></table></div>')
            table = False
        heading = re.match(r'^(#{1,4}) (.+)', line)
        if heading:
            flush()
            close_list()
            level = min(4, len(heading[1]) + 1)
            slug = re.sub(r'[^a-z0-9]+', '-', heading[2].lower()).strip('-')
            out.append(f'<h{level} id="{slug}">{inline(heading[2])}</h{level}>')
            continue
        item = re.match(r'^(?:([-*]) |(\d+)\. )(.+)', line)
        if item:
            flush()
            kind = 'ul' if item[1] else 'ol'
            if listing != kind:
                close_list()
                listing = kind
                out.append('<' + kind + '>')
            out.append('<li>' + inline(item[3]) + '</li>')
            continue
        if not line.strip():
            flush()
            continue
        close_list()
        paragraph.append(line)
    flush()
    close_list()
    if table:
        out.append('</tbody></table></div>')
    return '\n'.join(out)


tasks = json.loads((ROOT / 'agent-index.json').read_text())
families = json.loads((ROOT / 'evidence/handover/creative-qa/acceptance-matrix.json').read_text())['acceptance_families']
open_families = [family for family in families if 'continuation_task' in family]
page = '''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Civ III renderer handover</title><style>
:root{color-scheme:light dark;font-family:var(--font-sans,system-ui,sans-serif);line-height:1.55}
body{margin:0;color:var(--foreground,CanvasText);background:transparent}
main{max-width:1100px;margin:auto;padding:0}
h1,h2,h3,h4{line-height:1.25;scroll-margin-top:1rem}h1{font-size:1.8rem}h2{margin-top:2rem}h3{margin-top:1.7rem}
p,li{max-width:88ch}a{color:var(--primary,LinkText);text-underline-offset:3px}
nav{display:flex;flex-wrap:wrap;gap:.6rem 1.2rem;margin:1rem 0 1.5rem}
table{border-collapse:collapse;width:100%;font-size:.92rem}th,td{padding:.65rem;text-align:left;vertical-align:top;border-bottom:1px solid var(--border,GrayText)}
th{font-weight:650}td:first-child{min-width:8rem}.table-scroll{overflow:auto;margin:1rem 0}
code{font-family:var(--font-mono,monospace);font-size:.87em;overflow-wrap:anywhere}pre{white-space:pre-wrap;overflow-wrap:anywhere;padding:1rem;background:var(--code-background,ButtonFace);border-radius:8px}
input{box-sizing:border-box;width:100%;font:inherit;padding:.65rem .8rem;border:1px solid var(--border,GrayText);border-radius:6px;color:inherit;background:var(--input,Field)}
details{border-bottom:1px solid var(--border,GrayText);padding:.7rem 0}summary{cursor:pointer;font-weight:600;overflow-wrap:anywhere}.detail{padding:.2rem 1rem .8rem}
.muted{color:var(--muted-foreground,GrayText)}.tag{font-size:.85rem;font-weight:400;margin-left:.7rem}.hidden{display:none}
@media(max-width:600px){th,td{padding:.45rem}.detail{padding-left:.3rem;padding-right:.3rem}h1{font-size:1.5rem}}
@media print{input,nav{display:none}details>*{display:block}.table-scroll{overflow:visible}a{color:inherit}}
</style></head><body><main>
<h1>Civ III renderer handover</h1><p class="muted">22 October 2026 · Implementation baseline 8106ae5 · Unfinished continuation tasks</p>
<nav aria-label="Sections"><a href="#overview">Current status</a><a href="#agents">Unfinished tasks</a><a href="#acceptance">Acceptance reference</a><a href="#deferred">Deferred details</a><a href="#backups">Restore</a></nav>
'''
overview = (ROOT / 'README.md').read_text().split('\n', 1)[1]
page += '<section id="overview">' + markdown(overview) + '</section>'
page += '''<section id="agents"><h2>Unfinished tasks by priority</h2><p>Only still-unfinished work appears here. Continue an already-active agent without duplicating it. When resuming other work, use its brief and priority. Do not start the entire backlog or recreate completed and accepted agents. Sound and the frame target are P3; six-bit/seven-bit attribution is P4. Installation does not establish a working native window. <a href="subagents.md">Continuation instructions</a>.</p>
<label for="agent-search">Filter unfinished tasks</label><input id="agent-search" type="search" placeholder="For example: audio, combat, camera or performance" aria-controls="agent-list"><p id="agent-count" class="muted" aria-live="polite"></p><div id="agent-list">'''
for task in tasks:
    page += '<details data-filter><summary>' + html.escape(task['priority'] + ' · ' + task['title']) + '</summary><div class="detail"><p>' + html.escape(task['spawn_instruction']) + '</p><p>' + html.escape(task['brief']) + '</p><p>First action: ' + html.escape(task['first_step']) + '</p><p>Completion condition: ' + html.escape(task['done_when']) + '</p><p>Resources: ' + html.escape(task['resources']) + '</p>'
    if task['depends_on']:
        page += '<p>Dependent execution waits for: ' + html.escape(', '.join(task['depends_on'])) + '.</p>'
    page += '<p><a href="subagents.md#' + task['task_name'] + '">Starting material</a></p></div></details>'
page += '</div></section><section id="acceptance"><h2>Checks assigned to unfinished work</h2><p>These checks belong to actual unfinished tasks. The older feature ledger is a reference and does not reopen completed work. <a href="acceptance.md">Historical checklist</a>.</p><label for="family-search">Filter assigned checks</label><input id="family-search" type="search" placeholder="For example: combat, sound or 240" aria-controls="family-list"><p id="family-count" class="muted" aria-live="polite"></p><div id="family-list">'
for f in open_families:
    page += '<details data-filter><summary>' + html.escape(f['id']) + '<span class="tag">' + html.escape(f['priority'] + ' · ' + f['acceptance_status'].replace('_', ' ')) + '</span></summary><div class="detail"><p>' + html.escape(f['requirement']) + '</p>'
    nt = f.get('next_test', {})
    if isinstance(nt, dict):
        page += '<ol>' + ''.join('<li>' + html.escape(s) + '</li>' for s in nt.get('actions', [])) + '</ol>'
        if nt.get('oracle'):
            page += '<p>Acceptance oracle: ' + html.escape(nt['oracle']) + '</p>'
    page += '<p>When this work is resumed, spawn the <code>' + html.escape(f['continuation_task']) + '</code> subagent.</p><p><a href="acceptance.md">Historical test reference</a></p></div></details>'
page += '</div></section>'
for name, ident in [('verification.md', 'verification'), ('backup-and-restore.md', 'backups')]:
    page += '<section id="' + ident + '">' + markdown((ROOT / name).read_text()) + '</section>'
page += '<section id="deferred"><details><summary>Deferred P3 and P4 technical details</summary>' + markdown((ROOT / 'deferred-work.md').read_text()) + '</details></section>'
page += '''<p><a href="evidence-index.md">Starting evidence for unfinished tasks</a> · <a href="state.json">Machine-readable state</a></p></main><script>
function connect(inputId,listId,countId){const input=document.getElementById(inputId),list=document.getElementById(listId),rows=[...list.querySelectorAll('[data-filter]')],count=document.getElementById(countId);function update(){const q=input.value.toLocaleLowerCase().trim();let shown=0;for(const row of rows){const show=!q||row.textContent.toLocaleLowerCase().includes(q);row.hidden=!show;if(show)shown++;}count.textContent=shown+' of '+rows.length+' entries';}input.addEventListener('input',update);update();}
connect('agent-search','agent-list','agent-count');connect('family-search','family-list','family-count');
console.log('Handover entries',document.querySelectorAll('#agent-list details').length,document.querySelectorAll('#family-list details').length);
</script></body></html>'''
(ROOT / 'handover.html').write_text(page)
print(f'Wrote handover.html, {len(page.encode())} bytes, {len(tasks)} unfinished continuation tasks, {len(open_families)} open acceptance families.')
