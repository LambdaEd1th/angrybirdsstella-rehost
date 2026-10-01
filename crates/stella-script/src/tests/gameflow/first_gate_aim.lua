-- Read-only input selection through original raycast; never assigns world state.
return function(b, goal)
    local vx,vy=getLinearVelocity(b.name)
    local dt=getDeltaTimeMultiplier()/60
    local bx,by=b.x+vx*dt,b.y+vy*dt
    local cfg=getObjectDefinition(b.name).components.stella
    local function cast(x,y,dx,dy,distance)
        return raycast(x,y,x+dx*distance,y+dy*distance,{b},function(o)return not o.ignoreStellaAimHit end)
    end
    local function score(h)
        if not h then return 0 end
        local o=h.object
        if o==goal then return 20000 end
        -- The platform can shield the pig from this fruit even after a hit.
        if o.name=='BOMB_FRUIT_1' then return 0 end
        if levelGoals[o.name] then return 10000 end
        local distance=vLength(o.x-goal.x,o.y-goal.y)
        if distance<0.7 and o.material~='decoration' and not string.find(o.name,'HOMETREE') and not string.find(o.name,'STATIC') then return 14000 end
        return 0
    end
    local best=nil
    local run=nil
    for i=-360,360 do
        local angle=math.rad(i/2)
        local dx,dy=math.cos(angle),math.sin(angle)
        local first=cast(bx,by,dx,dy,cfg.maxDistance)
        local second=nil
        if first and first.object.material~='grabbable' and not contains(cfg.stoppingMaterials,first.object.material) then
            local nx,ny=first.rayCastNormalX,first.rayCastNormalY
            if first.object.type=='circle' then
                local step=math.rad(cfg.circleAngleStep)
                nx,ny=vec2FromAngle(math.floor((math.atan2(ny,nx)+0.5*step)/step)*step)
            end
            local x=first.rayCastContactX+first.rayCastNormalX*b.radius
            local y=first.rayCastContactY+first.rayCastNormalY*b.radius
            local remaining=cfg.maxDistance-vLength(x-bx,y-by)
            local rx,ry=vec2Reflect(dx,dy,nx,ny)
            if remaining>0 then second=cast(x,y,rx,ry,remaining) end
        end
        local rank=math.max(score(first),score(second))
        local key=(first and first.object.name or '')..'/'..(second and second.object.name or '')
        if rank>0 then
            if not run or run.key~=key or run.rank~=rank then run={key=key,rank=rank,first=i,last=i} else run.last=i end
            if not best or run.rank>best.rank or (run.rank==best.rank and run.last-run.first>best.last-best.first) then best={key=run.key,rank=run.rank,first=run.first,last=run.last} end
        else run=nil end
    end
    if not best then return nil end
    local angle=math.rad((best.first+best.last)/4)
    for distance=9,1,-0.5 do
        local x,y=bx+math.cos(angle)*distance,by+math.sin(angle)*distance
        local sx,sy=physicsToScreenTransform(x,y)
        if sx>=16 and sx<1008 and sy>=16 and sy<752 then return x,y,best.key,best.rank end
    end
    return nil
end
