import sys;sys.path.insert(0,'.')
import wordmark as w, re, subprocess
INK="#0A1F1E";PAPER="#F4F7F5";BRIGHT="#17A398"
src=open('icon.svg').read().split('>',1)[1].rsplit('</svg>',1)[0]
SQ=re.search(r'SQ="(M[0-9].*?)"',open('gen.py').read()).group(1)
def sv(b,W,H): return f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">{b}</svg>'
D=w.d()
def wm(col): return f'<path fill="{col}" fill-rule="evenodd" d="{D}"/>'
open('wordmark.svg','w').write(sv(wm(INK),w.W,w.H))
open('wordmark-dark.svg','w').write(sv(wm(PAPER),w.W,w.H))
def lock(col,T=1):
    # tile height = 1.9 x total wordmark height; wordmark x-height centred on tile
    tile=w.H*1.15; s=tile/1024; gap=tile*0.28
    ty=0; wy=tile/2-(w.XH[0]+w.XH[1])/2
    b=f'<g transform="scale({s:.4f})"><defs><clipPath id="lk{T}"><path d="{SQ}"/></clipPath></defs><g clip-path="url(#lk{T})">{src.replace(chr(34)+"g"+chr(34),chr(34)+"g"+str(T)+chr(34)).replace("url(#g)","url(#g"+str(T)+")").replace(chr(34)+"h"+chr(34),chr(34)+"hh"+str(T)+chr(34)).replace("url(#h)","url(#hh"+str(T)+")")}</g></g><g transform="translate({tile+gap:.1f} {wy:.1f})">{wm(col)}</g>'
    return b, tile+gap+w.W, max(tile,wy+w.H)
for n,c in [('lockup.svg',INK),('lockup-dark.svg',PAPER)]:
    b,W,H=lock(c); open(n,'w').write(sv(b,W,H))
for n,c in [('readme-header-light.svg',INK),('readme-header-dark.svg',PAPER)]:
    b,W,H=lock(c,2); s=min(600/W,140/H)
    open(n,'w').write(sv(f'<g transform="translate({(640-W*s)/2:.1f} {(160-H*s)/2:.1f}) scale({s:.4f})">{b}</g>',640,160))
r=lambda a,o,h:subprocess.run(["rsvg-convert","-h",str(h),a,"-o",o],check=True)
r('wordmark.svg','wordmark.png',400);r('wordmark-dark.svg','wordmark-dark.png',400)
r('lockup.svg','lockup.png',400);r('lockup-dark.svg','lockup-dark.png',400)
r('wordmark.svg','/tmp/wm24.png',24);r('lockup.svg','/tmp/lk24.png',24)
