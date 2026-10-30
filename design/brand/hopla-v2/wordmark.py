from shapely.geometry import LineString,Point
from shapely.ops import unary_union
K=64; R=K/2
def bez(p0,p1,p2,p3,n=40):
    return [tuple((1-t)**3*a+3*(1-t)**2*t*b+3*(1-t)*t*t*c+t**3*d for a,b,c,d in zip(p0,p1,p2,p3)) for t in [i/n for i in range(n+1)]]
def ring(cx,cy,r): return Point(cx,cy).buffer(r+R,256).difference(Point(cx,cy).buffer(r-R,256))
def ln(pts): return LineString(pts).buffer(R,64)
# metrics (centerline): ascender 72 (edge 40), x-height 182 (edge 150), baseline 418 (edge 450), descender 540 (edge 572)
# rounds: r=126 -> edges 148/452 (2px optical overshoot), ascender h/l identical
r=126; cy=300
g=[]
# h
g.append(ln([(60,72),(60,418)]))
g.append(ln([(60,300)]+bez((60,300),(60,222),(118,174),(182,174))[1:]+bez((182,174),(246,174),(304,222),(304,300))[1:]+[(304,418)]))
# o
ox=304+R+44+R+r-R; g.append(ring(ox,cy,r))                # round-straight gap 44
# p
px=ox+r+R+40+R-R-R+R; px=ox+r+R+40+R  # stem centre; round-straight gap 40
g.append(ln([(px,182),(px,540)])); g.append(ring(px+r,cy,r))
# l with hop tail (nod to the mark's lift-off)
lx=px+2*r+R+46+R
g.append(ln([(lx,72),(lx,330)]+bez((lx,330),(lx,392),(lx+30,418),(lx+78,418))[1:]))
# a
ax=lx+78+R+26+R+r-R+R; ax=lx+78+R+30+R+r-R
g.append(ring(ax,cy,r)); g.append(ln([(ax+r,182),(ax+r,418)]))
shape=unary_union(g)
minx,miny,maxx,maxy=shape.bounds
def d(dx=-minx,dy=-miny,s=1):
    out=[]
    polys=getattr(shape,'geoms',[shape])
    for p in polys:
        for c in [p.exterior,*p.interiors]:
            pts=list(c.coords)
            out.append("M"+"L".join(f"{(x+dx)*s:.1f} {(y+dy)*s:.1f}" for x,y in pts)+"Z")
    return "".join(out)
W,H=maxx-minx,maxy-miny
XH=(150-miny,450-miny)  # x-height band in local coords
