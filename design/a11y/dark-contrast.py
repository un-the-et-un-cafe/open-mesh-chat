# Measure every rendered dark-mode fg/bg pair in .phone.dark frames (and light, for regression check).
# usage: contrast.py MOCKUPS_DIR OUT.json [dark|light]
import sys,json,glob,os
from playwright.sync_api import sync_playwright
D,OUT=sys.argv[1],sys.argv[2]; MODE=sys.argv[3] if len(sys.argv)>3 else 'dark'
JS=r'''(mode)=>{
const cv=document.createElement('canvas');cv.width=cv.height=1;const cx=cv.getContext('2d',{willReadFrequently:true});
const memo={};
function P(s){if(!s||s==='none')return null;if(s in memo)return memo[s];if(/url\(/.test(s)){memo[s]=null;return null}cx.clearRect(0,0,1,1);cx.fillStyle='#010203';cx.fillStyle=s;if(cx.fillStyle==='#010203'&&s!=='#010203'){memo[s]=null;return null}cx.fillRect(0,0,1,1);const d=cx.getImageData(0,0,1,1).data;
 // fillRect composites over transparent: recover straight alpha
 const a=d[3]/255;const r=a?[d[0],d[1],d[2],a]:[0,0,0,0];memo[s]=r;return r}
function over(f,b,op){const a=f[3]*op;return [f[0]*a+b[0]*(1-a),f[1]*a+b[1]*(1-a),f[2]*a+b[2]*(1-a),1]}
function cols(img){return (img.match(/(rgba?\([^)]*\)|color\([^)]*\)|oklab\([^)]*\)|oklch\([^)]*\))/g)||[]).map(P).filter(Boolean)}
function chain(el,stop){const c=[];for(let e=el;e;e=e.parentElement){c.unshift(e);if(e===stop)break}return c}
const out=[];
const phones=[...document.querySelectorAll('.phone')].filter(p=>mode==='dark'?p.classList.contains('dark'):!p.classList.contains('dark'));
phones.forEach((ph,pi)=>{
 const st=ph.getAttribute('data-state')||String(pi);
 function bgOf(el){ // returns list of candidate composited bg colours (stripes => several)
  let bgs=[[255,255,255,1]];let op=1;
  for(const e of chain(el,ph)){const cs=getComputedStyle(e);op*=parseFloat(cs.opacity);
   const c=P(cs.backgroundColor);if(c&&c[3]>0)bgs=bgs.map(b=>over(c,b,op));
   const gi=cs.backgroundImage;if(gi&&gi!=='none'&&gi.includes('gradient')){const gc=cols(gi);if(gc.length){const nb=[];bgs.forEach(b=>gc.forEach(g=>nb.push(over(g,b,op))));bgs=nb}}}
  return bgs}
 function opOf(el){let op=1;for(const e of chain(el,ph))op*=parseFloat(getComputedStyle(e).opacity);return op}
 function vis(el){const r=el.getBoundingClientRect();const pr=ph.getBoundingClientRect();if(r.width<1||r.height<1)return false;if(r.bottom<pr.top||r.top>pr.bottom)return false;
  for(let e=el;e&&e!==ph;e=e.parentElement){const cs=getComputedStyle(e);if(cs.visibility==='hidden'||cs.display==='none')return false}return true}
 function desc(el){let s=el.tagName.toLowerCase();if(el.className&&typeof el.className==='string')s+='.'+el.className.trim().split(/\s+/).join('.');
  const p=el.parentElement;if(p&&p.className&&typeof p.className==='string')s=p.className.trim().split(/\s+/)[0]+'>'+s;return s}
 function disabled(el){for(let e=el;e&&e!==ph;e=e.parentElement){if(e.hasAttribute('disabled')||e.getAttribute('aria-disabled')==='true'||(e.classList&&e.classList.contains('disabled')))return true;if(parseFloat(getComputedStyle(e).opacity)<0.7)return true}return false}
 const all=ph.querySelectorAll('*');
 all.forEach(el=>{
  if(!vis(el))return;const cs=getComputedStyle(el);
  const tag=el.tagName.toLowerCase();
  // text
  const txt=[...el.childNodes].filter(n=>n.nodeType===3&&n.textContent.trim()).map(n=>n.textContent.trim()).join(' ');
  if(txt&&!el.closest('svg')){const fg=P(cs.color);const fs=parseFloat(cs.fontSize),fw=parseInt(cs.fontWeight);
   const large=fs>=24||(fs>=18.66&&fw>=700);
   out.push({st,k:disabled(el)?'text-disabled':(large?'text-large':'text'),fg,op:opOf(el),bgs:bgOf(el),el:desc(el),t:txt.slice(0,40)})}
  // icons
  if(tag==='svg'){let st2=null;for(const sh of el.querySelectorAll('path,circle,rect,ellipse,line,polyline,polygon')){const sc=getComputedStyle(sh);if(sc.stroke!=='none'&&parseFloat(sc.strokeWidth)>0){st2=P(sc.stroke);break}if(sc.fill!=='none'){if(/url\(/.test(sc.fill)){st2=null;break}const f=P(sc.fill);if(f&&f[3]>0){st2=f;break}}}const r=el.getBoundingClientRect();
   if(st2&&st2[3]>0&&r.width>=8){
    // ignore brand logos/illustrations with hard-coded fills
    if(el.closest('[data-brand],.logo')) return;
    out.push({st,k:disabled(el)?'icon-disabled':'icon',fg:st2,op:opOf(el),bgs:bgOf(el.parentElement),el:desc(el),t:''})}}
  // UI boundaries: controls only
  const ctrl=el.matches('.toggle,.checkbox,.field-box,.composer .field,.word.blank,.logo-tile,[role=textbox],[role=radio],.seg,[role=button][style*=border],.btn-secondary')||(tag==='button'&&parseFloat(cs.borderTopWidth)>0);
  if(ctrl){const bw=parseFloat(cs.borderTopWidth);const pbg=bgOf(el.parentElement);
   if(el.matches('.toggle')){out.push({st,k:disabled(el)?'ui-disabled':'ui',fg:P(cs.backgroundColor),op:opOf(el),bgs:pbg,el:desc(el)+(el.classList.contains('on')?'(on track)':'(off track)'),t:''});
     // thumb vs track
     const th=P(getComputedStyle(el,'::after').backgroundColor);if(th)out.push({st,k:'ui',fg:th,op:opOf(el),bgs:[over(P(cs.backgroundColor),pbg[0],opOf(el))],el:desc(el)+'(thumb vs track)',t:''})}
   else if(bw>0&&cs.borderTopStyle!=='none'){const own=P(cs.backgroundColor);const ownc=(own&&own[3]>0)?own:null;out.push({st,k:disabled(el)?'ui-disabled':'ui',fg:P(cs.borderTopColor),alt:ownc,op:opOf(el),bgs:pbg,el:desc(el)+'(border)',t:''})}}
  if(cs.outlineStyle!=='none'&&parseFloat(cs.outlineWidth)>0){out.push({st,k:'focus',fg:P(cs.outlineColor),op:opOf(el),bgs:bgOf(el.parentElement),el:desc(el)+'(focus ring)',t:''})}
 });
});
return out}'''
def lum(c):
    def f(v):
        v/=255; return v/12.92 if v<=0.03928 else ((v+0.055)/1.055)**2.4
    return 0.2126*f(c[0])+0.7152*f(c[1])+0.0722*f(c[2])
def ratio(a,b):
    la,lb=lum(a),lum(b); return (max(la,lb)+.05)/(min(la,lb)+.05)
def over(f,b,op):
    a=f[3]*op; return [f[i]*a+b[i]*(1-a) for i in range(3)]+[1]
hx=lambda c:'#%02x%02x%02x'%tuple(round(v) for v in c[:3])
rows=[]
with sync_playwright() as p:
    b=p.chromium.launch(executable_path='/usr/bin/google-chrome'); pg=b.new_page(viewport={'width':520,'height':1000},color_scheme='light')
    for f in sorted(glob.glob(D+'/*.html')):
        n=os.path.basename(f)[:-5]; pg.goto('file://'+f); pg.wait_for_timeout(50)
        for i,r in enumerate(pg.evaluate(JS,MODE)):
            if not r['fg'] or r['fg'][3]==0: continue
            worst=None
            for bg in r['bgs']:
                fg=over(r['fg'],bg,r['op']); q=ratio(fg,bg)
                if r.get('alt'): q=max(q,ratio(over(r['alt'],bg,r['op']),bg))  # filled control: fill also marks the boundary
                if worst is None or q<worst[0]: worst=(q,hx(fg),hx(bg))
            rows.append(dict(file=n,state=r['st'],idx=i,kind=r['k'],el=r['el'],text=r['t'],ratio=round(worst[0],2),fg=worst[1],bg=worst[2],rawfg=hx(r['fg'])))
    b.close()
json.dump(rows,open(OUT,'w')); print(len(rows),'measurements')
