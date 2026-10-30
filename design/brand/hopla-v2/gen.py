import re
import subprocess,os
P=dict(deep="#063F3D",core="#0B6E6A",bright="#17A398",mint="#7FE0CF",sky="#8CC4F5",ink="#0A1F1E",paper="#F4F7F5")
S=132 # master stroke
def glyph(col="#fff",node=None,gw=None,sw=S):
    node=node or col; gw=gw or P['sky']
    return f'''<g fill="none" stroke-linecap="round" stroke-linejoin="round">
<path d="M352 262V762" stroke="{col}" stroke-width="{sw}"/>
<path d="M352 560C352 430 430 372 520 372S688 430 688 560V600" stroke="{col}" stroke-width="{sw}"/>
</g><circle cx="688" cy="752" r="{sw*0.62:.0f}" fill="{node}"/><circle cx="760" cy="262" r="{sw*0.38:.0f}" fill="{gw}"/>'''
def bg(dark=False):
    a,b=(("#0F2A29","#050F0F") if dark else (P['bright'],P['deep']))
    return f'''<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{a}"/><stop offset=".55" stop-color="{P['core'] if not dark else '#0A1C1B'}"/><stop offset="1" stop-color="{b}"/></linearGradient>
<radialGradient id="h" cx=".3" cy=".2" r=".8"><stop offset="0" stop-color="#fff" stop-opacity=".14"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient></defs>
<rect width="1024" height="1024" fill="url(#g)"/><rect width="1024" height="1024" fill="url(#h)"/>'''
def svg(body,w=1024,h=1024): return f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{body}</svg>'
SQ="M512 0C860 0 1024 164 1024 512S860 1024 512 1024 0 860 0 512 164 0 512 0Z" # superellipse approx
SQ="M0 512C0 90 90 0 512 0S1024 90 1024 512 934 1024 512 1024 0 934 0 512Z"
SQ="M1024.0 512.0L1023.9 631.2L1023.7 669.3L1023.4 697.0L1022.9 719.5L1022.2 738.8L1021.5 755.8L1020.6 771.1L1019.5 785.2L1018.3 798.1L1016.9 810.2L1015.5 821.5L1013.8 832.1L1012.0 842.1L1010.1 851.6L1008.0 860.7L1005.8 869.3L1003.4 877.5L1000.9 885.3L998.2 892.8L995.4 900.0L992.4 906.9L989.2 913.5L985.9 919.9L982.4 926.0L978.7 931.8L974.9 937.4L970.9 942.8L966.7 948.0L962.3 953.0L957.7 957.7L953.0 962.3L948.0 966.7L942.8 970.9L937.4 974.9L931.8 978.7L926.0 982.4L919.9 985.9L913.5 989.2L906.9 992.4L900.0 995.4L892.8 998.2L885.3 1000.9L877.5 1003.4L869.3 1005.8L860.7 1008.0L851.6 1010.1L842.1 1012.0L832.1 1013.8L821.5 1015.5L810.2 1016.9L798.1 1018.3L785.2 1019.5L771.1 1020.6L755.8 1021.5L738.8 1022.2L719.5 1022.9L697.0 1023.4L669.3 1023.7L631.2 1023.9L512.0 1024.0L392.8 1023.9L354.7 1023.7L327.0 1023.4L304.5 1022.9L285.2 1022.2L268.2 1021.5L252.9 1020.6L238.8 1019.5L225.9 1018.3L213.8 1016.9L202.5 1015.5L191.9 1013.8L181.9 1012.0L172.4 1010.1L163.3 1008.0L154.7 1005.8L146.5 1003.4L138.7 1000.9L131.2 998.2L124.0 995.4L117.1 992.4L110.5 989.2L104.1 985.9L98.0 982.4L92.2 978.7L86.6 974.9L81.2 970.9L76.0 966.7L71.0 962.3L66.3 957.7L61.7 953.0L57.3 948.0L53.1 942.8L49.1 937.4L45.3 931.8L41.6 926.0L38.1 919.9L34.8 913.5L31.6 906.9L28.6 900.0L25.8 892.8L23.1 885.3L20.6 877.5L18.2 869.3L16.0 860.7L13.9 851.6L12.0 842.1L10.2 832.1L8.5 821.5L7.1 810.2L5.7 798.1L4.5 785.2L3.4 771.1L2.5 755.8L1.8 738.8L1.1 719.5L0.6 697.0L0.3 669.3L0.1 631.2L0.0 512.0L0.1 392.8L0.3 354.7L0.6 327.0L1.1 304.5L1.8 285.2L2.5 268.2L3.4 252.9L4.5 238.8L5.7 225.9L7.1 213.8L8.5 202.5L10.2 191.9L12.0 181.9L13.9 172.4L16.0 163.3L18.2 154.7L20.6 146.5L23.1 138.7L25.8 131.2L28.6 124.0L31.6 117.1L34.8 110.5L38.1 104.1L41.6 98.0L45.3 92.2L49.1 86.6L53.1 81.2L57.3 76.0L61.7 71.0L66.3 66.3L71.0 61.7L76.0 57.3L81.2 53.1L86.6 49.1L92.2 45.3L98.0 41.6L104.1 38.1L110.5 34.8L117.1 31.6L124.0 28.6L131.2 25.8L138.7 23.1L146.5 20.6L154.7 18.2L163.3 16.0L172.4 13.9L181.9 12.0L191.9 10.2L202.5 8.5L213.8 7.1L225.9 5.7L238.8 4.5L252.9 3.4L268.2 2.5L285.2 1.8L304.5 1.1L327.0 0.6L354.7 0.3L392.8 0.1L512.0 0.0L631.2 0.1L669.3 0.3L697.0 0.6L719.5 1.1L738.8 1.8L755.8 2.5L771.1 3.4L785.2 4.5L798.1 5.7L810.2 7.1L821.5 8.5L832.1 10.2L842.1 12.0L851.6 13.9L860.7 16.0L869.3 18.2L877.5 20.6L885.3 23.1L892.8 25.8L900.0 28.6L906.9 31.6L913.5 34.8L919.9 38.1L926.0 41.6L931.8 45.3L937.4 49.1L942.8 53.1L948.0 57.3L953.0 61.7L957.7 66.3L962.3 71.0L966.7 76.0L970.9 81.2L974.9 86.6L978.7 92.2L982.4 98.0L985.9 104.1L989.2 110.5L992.4 117.1L995.4 124.0L998.2 131.2L1000.9 138.7L1003.4 146.5L1005.8 154.7L1008.0 163.3L1010.1 172.4L1012.0 181.9L1013.8 191.9L1015.5 202.5L1016.9 213.8L1018.3 225.9L1019.5 238.8L1020.6 252.9L1021.5 268.2L1022.2 285.2L1022.9 304.5L1023.4 327.0L1023.7 354.7L1023.9 392.8Z"
W=lambda n,s:open(n,'w').write(s)
icon=svg(bg()+glyph(node=P['mint']))
W("icon.svg",icon)
W("icon-preview-rounded.svg",svg(f'<defs><clipPath id="c"><path d="{SQ}"/></clipPath></defs><g clip-path="url(#c)">{bg()+glyph(node=P["mint"])}</g>'))
W("icon-dark.svg",svg(bg(True)+glyph(col=P['mint'],node="#fff")))
W("icon-tinted.svg",svg('<rect width="1024" height="1024" fill="#000"/>'+glyph(col="#fff",node="#fff",gw="#9a9a9a")))
# android: 108dp canvas -> 1024; safe zone 66/108 => glyph scaled .6 centered
ad=lambda body:f'<g transform="translate(512 512) scale(.6) translate(-520 -512)">{body}</g>'
W("adaptive-foreground.svg",svg(ad(glyph(node=P['mint']))))
W("adaptive-background.svg",svg(bg()))
W("monochrome.svg",svg(ad(glyph(col="#000",node="#000",gw="#000"))))
W("glyph.svg",svg(f'<g transform="translate(-200 -150)">{glyph(col=P["core"],node=P["bright"],gw=P["sky"])}</g>',624,724))
# wordmark: custom monoline letters on x-height 300, stroke 64
k=64
def word(col,dot):
    L=f'''<g fill="none" stroke="{col}" stroke-width="{k}" stroke-linecap="round">
<path d="M40 40V440"/><path d="M40 300C40 200 90 150 160 150S280 200 280 300V440"/>
<circle cx="490" cy="295" r="145"/>
<path d="M720 160V560"/><path d="M720 295A145 145 0 1 1 720 296"/>
<path d="M1010 40V380Q1010 440 1060 440"/>
<circle cx="1260" cy="295" r="145"/><path d="M1405 160V440"/></g>
<circle cx="1112" cy="80" r="40" fill="{dot}"/>'''
    return L
def r(s,o,w): subprocess.run(["rsvg-convert","-w",str(w),s,"-o",o],check=True)
for n in [1024,180,120,87,58,48,29]: r("icon.svg",f"icon-{n}.png",n)
# explorations
def tile(i,body,lbl):
    x=(i%3)*540;y=(i//3)*600
    return f'<g transform="translate({x+20} {y+20}) scale(.48)"><rect width="1024" height="1024" rx="230" fill="{P["core"]}"/>{body}</g><text x="{x+266}" y="{y+560}" font-family="Sen" font-size="26" text-anchor="middle" fill="#333">{lbl}</text>'
C=[glyph(node=P['mint']),
'<g fill="none" stroke="#fff" stroke-width="90" stroke-linecap="round"><path d="M220 720Q360 300 512 720Q664 300 804 720"/></g><circle cx="220" cy="760" r="60" fill="#fff"/><circle cx="804" cy="760" r="60" fill="#fff"/>',
'<path d="M200 300h624v360H440l-160 120v-120h-80z" fill="#fff"/><circle cx="380" cy="480" r="50" fill="#0B6E6A"/><circle cx="644" cy="480" r="50" fill="#0B6E6A"/><path d="M380 480Q512 360 644 480" stroke="#0B6E6A" stroke-width="30" fill="none"/>',
'<g fill="none" stroke="#fff" stroke-width="70"><circle cx="410" cy="512" r="200"/><circle cx="614" cy="512" r="200"/></g>',
'<g fill="#fff"><rect x="252" y="252" width="200" height="520" rx="100"/><rect x="572" y="452" width="200" height="320" rx="100"/><circle cx="672" cy="300" r="70"/></g>',
'<g fill="none" stroke="#fff" stroke-width="60"><path d="M300 700A212 212 0 0 1 724 700"/><path d="M400 700A112 112 0 0 1 624 700"/></g><circle cx="512" cy="700" r="50" fill="#fff"/>']
L=["A · Hop-h monogram (WINNER)","B · Bounce trail (too Wi-Fi/'m')","C · Bubble+link (generic chat)","D · Peer rings (Mastercard-ish)","E · Grid hop glyph (runner-up)","F · Radar arcs (reads Wi-Fi)"]
W("explorations.svg",svg('<rect width="1620" height="1200" fill="#fafafa"/>'+"".join(tile(i,c,l) for i,(c,l) in enumerate(zip(C,L))),1620,1200))
r("explorations.svg","explorations.png",1620)
# construction
g=''.join(f'<line x1="{v}" y1="0" x2="{v}" y2="1024" stroke="#ff5a5a" stroke-opacity=".35" stroke-width="2"/><line y1="{v}" x1="0" y2="{v}" x2="1024" stroke="#ff5a5a" stroke-opacity=".35" stroke-width="2"/>' for v in range(0,1025,64))
circ='<g fill="none" stroke="#fff" stroke-opacity=".7" stroke-width="3" stroke-dasharray="10 8"><circle cx="512" cy="512" r="420"/><circle cx="512" cy="512" r="300"/><rect x="186" y="186" width="652" height="652"/><circle cx="520" cy="560" r="168"/><line x1="352" y1="0" x2="352" y2="1024"/><line x1="688" y1="0" x2="688" y2="1024"/><line x1="0" y1="262" x2="1024" y2="262"/><line x1="0" y1="752" x2="1024" y2="752"/></g>'
ann='<g font-family="Sen" font-size="22" fill="#fff"><text x="30" y="40">64 px grid · stroke 132 · arch r168 · node r82 (0.62S) · gateway r50 (0.38S)</text><text x="30" y="1000">Stem &amp; arch share one stroke; gap between arch end and node = 1 S (the hop)</text></g>'
W("construction.svg",svg(bg()+glyph(node=P['mint'])+g+circ+ann))
r("construction.svg","construction.png",1024)
# homescreen
apps=['<circle cx="512" cy="512" r="260" fill="none" stroke="#fff" stroke-width="80"/>','<rect x="262" y="312" width="500" height="400" rx="60" fill="#fff"/>','<path d="M312 712L512 312L712 712Z" fill="#fff"/>','<rect x="300" y="300" width="424" height="424" rx="212" fill="#fff" fill-opacity=".9"/><circle cx="512" cy="512" r="90" fill="#333"/>']
cols=["#F2994A","#5B6CFF","#E84A5F","#2D2D2D","#27AE60","#9B51E0","#F2C94C","#56CCF2"]
def hs(x0,y0,dark=False):
    out=f'<rect x="{x0}" y="{y0}" width="600" height="1100" rx="80" fill="{"#111" if dark else "url(#wp)"}"/>'
    k=0
    for rr in range(4):
        for c in range(4):
            x=x0+50+c*135;y=y0+120+rr*170
            if rr==1 and c==1: body=bg()+glyph(node=P['mint']);lbl="Hopla"
            else: body=f'<rect width="1024" height="1024" fill="{cols[k%8]}"/>'+apps[k%4];lbl="App";k+=1
            out+=f'<g transform="translate({x} {y}) scale(.1)"><defs><clipPath id="s{x}{y}"><path d="{SQ}"/></clipPath></defs><g clip-path="url(#s{x}{y})">{body}</g></g><text x="{x+51}" y="{y+130}" font-family="Sen" font-size="18" fill="#fff" text-anchor="middle">{lbl}</text>'
    return out
wp='<defs><linearGradient id="wp" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#C9B8A6"/><stop offset="1" stop-color="#5E6E7A"/></linearGradient></defs>'
W("homescreen.svg",svg(wp+hs(0,0),600,1100)); r("homescreen.svg","homescreen.png",600)
# presentation
def ri(src,x,y,s,rnd=True):
    b=open(src).read().split('>',1)[1].rsplit('</svg>',1)[0]
    cid=f"p{x}{y}"
    return f'<g transform="translate({x} {y}) scale({s/1024})"><defs><clipPath id="{cid}"><path d="{SQ}"/></clipPath></defs><g clip-path="url(#{cid})">{b}</g></g>'
pal=''.join(f'<rect x="{80+i*190}" y="1500" width="170" height="120" rx="16" fill="{v}" stroke="#ddd"/><text x="{80+i*190}" y="1650" font-family="Sen" font-size="22" fill="#333">{k} {v}</text>' for i,(k,v) in enumerate(P.items()))
sizes=''.join(ri("icon.svg",80+sum([180,120,87,58,48][:i])+i*40,1200+(180-s)//2,s) for i,s in enumerate([180,120,87,58,48,29]))
pres=svg(wp+f'<rect width="2400" height="1720" fill="{P["paper"]}"/><text x="80" y="110" font-family="Sen" font-weight="700" font-size="56" fill="{P["ink"]}">Hopla — app icon v2</text><text x="80" y="160" font-family="Sen" font-size="28" fill="#556">The hop-h: one stroke leaps, a peer lands, a gateway waits.</text>'
 +ri("icon.svg",80,220,860)+sizes+ri("icon-dark.svg",1000,220,300)+ri("icon-tinted.svg",1000,560,300)+ri("icon.svg",1340,220,300)
 +f'<text x="1000" y="900" font-family="Sen" font-size="24" fill="#556">light · dark · tinted</text><g transform="translate(1000 990) scale(.29)">{open("lockup.svg").read().split(">",1)[1].rsplit("</svg>",1)[0]}</g>'
 +pal+f'<g transform="translate(1720 220) scale(.9)">{hs(0,0)}</g>',2400,1720)
W("presentation.svg",pres); r("presentation.svg","presentation.png",2400)
