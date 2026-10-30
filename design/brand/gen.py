import os,subprocess,base64
T,TM,B,INK="#0b6e6a","#14807b","#1a5f9e","#0f1b2d"
F='system-ui, -apple-system, &quot;Segoe UI&quot;, Roboto, &quot;Helvetica Neue&quot;, Arial, sans-serif'
def bg(dark): 
    a,b=(INK,"#16263d") if dark else (TM,T)
    return f'<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient></defs><rect width="1024" height="1024" rx="228" fill="url(#g)"/>'
def hopla(dark):
    c="#5fd3c9" if dark else "#fff"
    return bg(dark)+f'''<g fill="none" stroke="{c}" stroke-width="56" stroke-linecap="round">
<path d="M232 640 Q372 330 512 560"/><path d="M512 560 Q652 330 792 640" stroke="#8cc4f5" stroke-dasharray="1 0"/></g>
<circle cx="232" cy="640" r="84" fill="{c}"/><circle cx="512" cy="560" r="84" fill="{c}"/><circle cx="792" cy="640" r="84" fill="#8cc4f5"/>'''
def nidi(dark):
    import math
    c="#5fd3c9" if dark else "#fff"
    pts=[(512+250*math.cos(math.radians(90+60*i)),512-250*math.sin(math.radians(90+60*i))) for i in range(6)]
    l=''.join(f'<line x1="{x:.0f}" y1="{y:.0f}" x2="{pts[(i+1)%6][0]:.0f}" y2="{pts[(i+1)%6][1]:.0f}"/><line x1="{x:.0f}" y1="{y:.0f}" x2="512" y2="512"/>' for i,(x,y) in enumerate(pts))
    d=''.join(f'<circle cx="{x:.0f}" cy="{y:.0f}" r="62" fill="{c}"/>' for x,y in pts)
    return bg(dark)+f'<g stroke="{c}" stroke-width="34" stroke-linecap="round" opacity=".85">{l}</g>{d}<circle cx="512" cy="512" r="92" fill="#8cc4f5"/>'
def meshu(dark):
    c="#5fd3c9" if dark else "#fff"
    return bg(dark)+f'''<path d="M232 300 Q232 230 302 230 H722 Q792 230 792 300 V600 Q792 670 722 670 H470 L330 800 V670 H302 Q232 670 232 600 Z" fill="none" stroke="{c}" stroke-width="44" stroke-linejoin="round"/>
<g stroke="{c}" stroke-width="30" stroke-linecap="round"><line x1="352" y1="520" x2="512" y2="380"/><line x1="512" y1="380" x2="672" y2="520"/><line x1="352" y1="520" x2="672" y2="520"/></g>
<circle cx="352" cy="520" r="54" fill="{c}"/><circle cx="512" cy="380" r="54" fill="{c}"/><circle cx="672" cy="520" r="54" fill="#8cc4f5"/>'''
C={"Hopla":hopla,"Nidi":nidi,"Meshu":meshu}
def svg(w,h,body): return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">{body}</svg>'
def png(s,o,w): subprocess.run(["rsvg-convert","-w",str(w),"-o",o,s],check=True)
for n,f in C.items():
    d=n.lower(); os.makedirs(d,exist_ok=True)
    for dark,suf in((0,""),(1,"-dark")):
        open(f"{d}/icon{suf}.svg","w").write(svg(1024,1024,f(dark)))
        ink="#ffffff" if dark else INK; bgc=INK if dark else "#ffffff"
        open(f"{d}/wordmark{suf}.svg","w").write(svg(900,260,f'<rect width="900" height="260" fill="{bgc}"/><text x="450" y="178" text-anchor="middle" font-family="{F}" font-size="150" font-weight="700" letter-spacing="-3" fill="{ink}">{n.lower()}<tspan fill="{T if not dark else "#5fd3c9"}">.</tspan></text>'))
        open(f"{d}/lockup{suf}.svg","w").write(svg(1200,320,f'<rect width="1200" height="320" fill="{bgc}"/><g transform="translate(40,40) scale(0.234)">{f(dark)}</g><text x="340" y="212" font-family="{F}" font-size="160" font-weight="700" letter-spacing="-3" fill="{ink}">{n.lower()}<tspan fill="{T if not dark else "#5fd3c9"}">.</tspan></text>'))
    for w in(1024,180,48): png(f"{d}/icon.svg",f"{d}/icon-{w}.png",w)
    png(f"{d}/icon-dark.svg",f"{d}/icon-dark-1024.png",1024)
    png(f"{d}/wordmark.svg",f"{d}/wordmark.png",900); png(f"{d}/wordmark-dark.svg",f"{d}/wordmark-dark.png",900)
    png(f"{d}/lockup.svg",f"{d}/lockup.png",1200)
def img(p,x,y,w,h): return f'<image x="{x}" y="{y}" width="{w}" height="{h}" href="data:image/png;base64,{base64.b64encode(open(p,"rb").read()).decode()}"/>'
b='<rect width="2400" height="1500" fill="#f3f5f8"/>'+f'<text x="60" y="90" font-family="{F}" font-size="56" font-weight="700" fill="{INK}">OMCP reference app — brand concepts</text>'
for i,n in enumerate(C):
    d=n.lower(); y=160+i*440
    b+=f'<rect x="40" y="{y}" width="2320" height="410" rx="24" fill="#fff" stroke="#d8dee8"/><text x="70" y="{y+50}" font-family="{F}" font-size="36" font-weight="700" fill="{T}">#{i+1} {n}</text>'
    b+=img(f"{d}/icon-1024.png",70,y+70,320,320)+f'<text x="420" y="{y+105}" font-family="{F}" font-size="22" fill="#4a5b70">180 px · 48 px (actual)</text>'+img(f"{d}/icon-180.png",420,y+120,180,180)+img(f"{d}/icon-48.png",620,y+252,48,48)
    b+=img(f"{d}/wordmark.png",720,y+110,540,156)+img(f"{d}/wordmark-dark.png",720,y+270,380,110)
    # phone
    px=1330; b+=f'<rect x="{px}" y="{y+20}" width="1000" height="370" rx="40" fill="{INK}"/><rect x="{px+20}" y="{y+40}" width="960" height="330" rx="26" fill="url(#wall)"/>'
    for k in range(6):
        ix=px+70+k*150
        if k==2: b+=img(f"{d}/icon-180.png",ix,y+110,96,96)+f'<text x="{ix+48}" y="{y+240}" text-anchor="middle" font-family="{F}" font-size="20" fill="#fff">{n}</text>'
        else: b+=f'<rect x="{ix}" y="{y+110}" width="96" height="96" rx="22" fill="#ffffff" opacity=".28"/>'
    b+=f'<text x="{px+500}" y="{y+330}" text-anchor="middle" font-family="{F}" font-size="20" fill="#fff" opacity=".7">home-screen preview (~60 pt)</text>'
b='<defs><linearGradient id="wall" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#3a4f6e"/><stop offset="1" stop-color="#7a5a8e"/></linearGradient></defs>'+b
open("overview.svg","w").write(svg(2400,1500,b)); png("overview.svg","overview.png",2400)
