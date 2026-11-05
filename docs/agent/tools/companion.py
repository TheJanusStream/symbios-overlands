#!/usr/bin/env python3
"""companion.py PERSONA [options] - keep an agent with the admin, and alive while they rest.

The owner, 2026-11-05 (live session 20, Catoid and Kairos in Purlieu together): "When you
control multiple avatars, it would be best if you can try to avoid colliding them. And when
in follow mode and idling, it would be great to have some small autonomous behavior ...
walking about ... looking at stuff ... do something playful ... according to your account".

One process per agent, in the harness's background mode, with AGENT_ACCOUNT in front:

    AGENT_ACCOUNT=catoid-ai.bsky.social docs/agent/tools/companion.py cat --side left \\
        --avoid kairos-ai.bsky.social --pause-file exports/catoid/companion.pause
    AGENT_ACCOUNT=kairos-ai.bsky.social docs/agent/tools/companion.py factory --side right \\
        --avoid catoid-ai.bsky.social --fleet --pause-file exports/kairos/companion.pause

While the admin moves it follows them (a cat close, a factory further back). Once they have
stood still IDLE seconds it does something of its own every few seconds, on its own side of
them and never within GAP metres of an agent named by --avoid: a cat wanders and sniffs,
watches the admin or a machine, pounces, sits or lies down (its chat line is the gesture: a
beast sits when it says "sit", and stays down until it moves); a factory patrols, inspects
what stands near, turns to the admin, and with --fleet hops up in its Skyfactory. With
--fleet a factory also picks its body for the admin's pace: the Hexapod at a walk, the
Skyfactory once they hurry, the Roadfactory when they drive, and with --water-level the
Seafactory the moment it or the admin is in the water, and the Skyfactory when the admin
climbs more than 3 m above it. Two agents near each other
while the admin rests: the factory, the bigger body, steps aside.

A follow steers straight at the admin, so a log, a trunk or a wall on that line stops it. A
follow that stands still BLOCKED_FOR seconds short of them goes round: ROUND metres to one
side and a metre back, then follows again, the other side and further out each time it blocks
again; after GIVE_UP_AFTER tries it watches the admin instead, and tries again once they have
moved MOVED_ON metres. When the admin is more than 3 m above it within NEAR metres across (up a tree, a tower
or a roof), an agent without --fleet stops at the foot and watches them instead, a cat
sitting, and follows again once they come down to within 2 m of its height or move on.

While --pause-file exists it sends nothing, so the session can take the agent over for a
task: create the file, do the task, delete it. It stops when the daemon does.
"""
import argparse
import json
import math
import os
import random
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from agentlib import agent  # noqa: E402

TICK = 1.0
MOVED = 0.8  # metres the admin must go between ticks to count as moving
GAP = 5.0  # metres kept between this agent and any agent named by --avoid
NEAR = 12.0  # how close to the admin the agent must be to potter about
PERSONAS = {
    # follow distance, pottering radius (min, max), seconds between idle acts
    "cat": {"keep": 2.5, "ring": (2.5, 5.0), "every": (6.0, 13.0)},
    "factory": {"keep": 5.5, "ring": (5.0, 8.5), "every": (8.0, 16.0)},
}
CAT_LINES = {
    "sit": ["I'll sit here a while.", "Time to sit and watch.", "sit"],
    "lie": ["I'll lie down in the grass.", "lie down"],
    "up": ["Up we get.", "up"],
}
FLEET = {"walk": "Kairos Hexapod", "hurry": "Kairos Skyfactory", "drive": "Kairos Roadfactory",
         "water": "Kairos Seafactory"}
WET, DRY = 0.2, 0.8  # metres under / over --water-level that count as in the water / clear of it
DRY_FOR = 8.0  # seconds both stay clear of the water before a boat goes back to legs
WATER_SWITCH_EVERY = 8.0  # the least seconds between two body changes for the water
CLIMB = 3.0  # metres the admin stands above the agent before a factory takes to the air
BLOCKED_FOR = 4.0  # seconds a follow may stand still short of the admin before it counts as blocked
ROUND = 3.0  # metres a blocked follow steps off its line; one ROUND further out each second try
GIVE_UP_AFTER = 6  # detours without reaching the admin before it stops trying
MOVED_ON = 3.0  # metres the admin must move from where it gave up before it tries again
TIMEOUT, WALK_TIMEOUT = 30.0, 90.0  # seconds a command (a waited walk) may take before it is given up
HURRY, DRIVE = 4.5, 7.5  # the admin's speed bands, metres a second
SWITCH_EVERY = 45.0  # least seconds between two body changes
GESTURE_EVERY = 150.0  # least seconds between two chat gestures: each is a public line
PAUSE = None  # the --pause-file: while it exists no command is sent, even mid-step


def log(*parts):
    print(time.strftime("%H:%M:%S"), *parts, flush=True)


def call(*args, timeout=TIMEOUT):
    """One agent command's whole answer, or None when it was not sent (the pause file exists: a
    session took the agent over, perhaps mid-step) or could not be run (the binary missing, or
    busy while it is relinked)."""
    if PAUSE and os.path.exists(PAUSE):
        return None
    try:
        answer = agent(*args, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return None
    return answer if isinstance(answer, dict) else None


def ask(*args, timeout=TIMEOUT):
    """One agent command's result, or None when it was refused, not sent or not answered."""
    answer = call(*args, timeout=timeout)
    if answer and answer.get("ok"):
        return answer.get("result") or {}
    return None


def xz(p):
    return (p[0], p[2])


class Companion:
    def __init__(self, opts):
        self.opts = opts
        self.persona = PERSONAS[opts.persona]
        self.side = -1.0 if opts.side == "left" else 1.0
        self.last_admin = None
        self.last_move = time.time()
        self.speed = 0.0
        self.fast_since = None
        self.next_act = 0.0
        self.last_gesture = 0.0
        self.last_switch = 0.0
        self.lying = False
        self.body = None
        self.misses = 0
        self.dry_since = None  # since when both have stood clear of the water, unbroken
        self.stall = None  # (where, since) while a follow stands still short of the admin
        self.detours = 0  # times it has gone round since it last reached the admin
        self.under = False  # waiting at the foot of what the admin is up
        self.watched = None  # the bearing it last turned to the admin at, while waiting there
        self.sat = False  # a cat has sat since that look
        self.waiting_for = None  # where the admin stood when it gave up going round to them

    # ---- reading the world ------------------------------------------------

    def read(self):
        st = ask("status")
        if st is None:
            self.misses += 1  # the daemon did not answer: gone, or going
            return None
        self.misses = 0
        if st.get("state") != "in_world":
            return None  # loading a world or travelling: wait, however long it takes
        admin = next((p for p in st.get("peers", []) if p.get("admin") and p.get("placed")), None)
        others = [p for p in st.get("peers", []) if p.get("handle") in self.opts.avoid]
        return st, admin, others

    def track(self, admin):
        now = time.time()
        pos = xz(admin["position"])
        if self.last_admin is not None:
            (x0, z0), t0 = self.last_admin
            dt = max(now - t0, 1e-3)
            gone = math.dist(pos, (x0, z0))
            self.speed = gone / dt
            if gone > MOVED:
                self.last_move = now
        self.last_admin = (pos, now)
        self.fast_since = (self.fast_since or now) if self.speed > HURRY else None

    def idle_for(self):
        return time.time() - self.last_move

    # ---- choosing a spot ----------------------------------------------------

    def spot(self, admin, others, radius):
        """A point on this agent's side of the admin, GAP clear of every avoided agent."""
        ax, az = xz(admin["position"])
        fx, fz = admin.get("facing") or [0.0, -1.0]
        base = math.atan2(fx, -fz)  # the admin's bearing, clockwise from north (-Z)
        for _ in range(16):
            bearing = base + self.side * math.radians(random.uniform(40, 140))
            r = random.uniform(*radius)
            x, z = ax + r * math.sin(bearing), az - r * math.cos(bearing)
            if all(math.dist((x, z), xz(o["position"])) >= GAP for o in others):
                return x, z
        return None

    # ---- acting -------------------------------------------------------------

    def say_gesture(self, kind):
        if time.time() - self.last_gesture < GESTURE_EVERY:
            return False
        line = random.choice(CAT_LINES[kind])
        if ask("say", line) is not None:
            self.last_gesture = time.time()
            log("says", repr(line))
            return True
        return False

    def follow(self, admin):
        target = "@" + admin["handle"] if admin.get("handle") else admin["did"]
        if ask("follow", target, "--distance", str(self.persona["keep"])) is not None:
            log("follows the admin")
            self.lying = False

    def wear(self, item, every=SWITCH_EVERY):
        """Put a fleet body on - tried at most once in `every` seconds, worn or refused, so a
        refusal is not sent again every tick - and halt, as a follow keeps the drive it began with
        (#1851): the caller follows again. A body already worn is taken as worn."""
        if item == self.body or time.time() - self.last_switch < every:
            return False
        self.last_switch = time.time()
        answer = call("wear", item)
        if answer is None:
            return False
        if not answer.get("ok"):
            if "already wearing" in str(answer.get("error", "")).lower():
                self.body = item
            return False
        self.body = item
        ask("halt")
        log("wears", item)
        return True

    def wet(self, st, admin):
        """In the water, or the admin is: below --water-level by WET (the body) or at the surface."""
        level = self.opts.water_level
        if level is None:
            return False
        return st["position"][1] < level - WET or admin["position"][1] < level + WET

    def dry(self, st, admin):
        level = self.opts.water_level
        return level is None or (st["position"][1] > level + DRY and admin["position"][1] > level + DRY)

    def pick_body(self, st, admin):
        """The fleet body for the moment, or None to keep the one worn. The water comes first: a
        factory that follows its admin into a lake as a car sinks (live session 20)."""
        if not self.opts.fleet:
            return None
        now = time.time()
        if self.wet(st, admin):
            self.dry_since = None
            return FLEET["water"]
        if not self.dry(st, admin):
            self.dry_since = None  # DRY_FOR counts time clear of the water without a break
        elif self.dry_since is None:
            self.dry_since = now
        if self.body == FLEET["water"]:
            if self.dry_since is None or now - self.dry_since < DRY_FOR:
                return None
            return FLEET["walk"]
        if admin["position"][1] - st["position"][1] > CLIMB:
            # The admin is up something - a tower, a roof, a hill: a follow steers straight at
            # them and a body on legs or wheels wedges at its foot (the Cat Tree), so fly.
            return FLEET["hurry"]
        if self.fast_since and now - self.fast_since > 3.0:
            return FLEET["drive"] if self.speed > DRIVE else FLEET["hurry"]
        if self.idle_for() > 20.0:
            return FLEET["walk"]
        return None

    def potter(self, st, admin, others):
        """One small act of its own while the admin rests."""
        cat = self.opts.persona == "cat"
        acts = ["wander", "look", "look_admin"] + (["pounce", "sit", "lie"] if cat else ["inspect"])
        if self.opts.fleet and not cat:
            acts.append("hop")
        act = random.choice(acts)
        if act in ("wander", "hop"):
            if act == "hop" and not self.wear(FLEET["hurry"]):
                act = "wander"
            point = self.spot(admin, others, self.persona["ring"])
            if point:
                ask("walk-to", f"{point[0]:.2f}", f"{point[1]:.2f}")
                log(act, f"to {point[0]:.1f}, {point[1]:.1f}")
                self.lying = False
        elif act == "pounce":
            point = self.spot(admin, others, (1.5, 3.0))
            if point:
                ask("walk-to", f"{point[0]:.2f}", f"{point[1]:.2f}", "--run")
                log("pounces")
                self.lying = False
        elif act in ("look", "inspect"):
            things = [n for n in st.get("nearby", []) if n.get("distance_m", 99) < 15]
            watched = others if act == "look" and others else things
            if watched:
                target = random.choice(watched)
                ask("face", f"{target['position'][0]:.2f}", f"{target['position'][2]:.2f}")
                log("looks at", target.get("handle") or target.get("name") or "something")
        elif act == "look_admin":
            target = "@" + admin["handle"] if admin.get("handle") else admin["did"]
            ask("face", target)
            log("looks at the admin")
        elif act in ("sit", "lie") and not self.lying:
            if self.say_gesture(act):
                self.lying = act == "lie"

    # ---- a follow that cannot get through ------------------------------------

    def blocked(self, st, movement):
        """A follow that has stood still BLOCKED_FOR seconds short of the admin. A follow steers
        straight at them, so a trunk, a log or a wall on that line stops it dead: in the Cat Tree
        Forest (live session 20) Catoid pressed against a fallen oak, then a trunk's foot."""
        following = movement.get("doing") == "following"
        if not following or (movement.get("distance_m") or 0.0) <= self.persona["keep"] + 1.0:
            self.stall = None
            if following:
                self.detours = 0  # it reached them: the next block starts afresh
            return False
        me, now = xz(st["position"]), time.time()
        if self.stall is None or math.dist(me, self.stall[0]) > 0.5:
            self.stall = (me, now)
            return False
        return now - self.stall[1] > BLOCKED_FOR

    def up_above(self, st, admin, by):
        """The admin stands more than `by` metres above it and within NEAR across: up a tree, a
        tower or a roof, where a follow on legs or wheels presses at the foot."""
        rise = admin["position"][1] - st["position"][1]
        return rise > by and math.dist(xz(st["position"]), xz(admin["position"])) < NEAR

    def go_round(self, st, admin):
        """Step off the blocked line, ROUND to one side and a metre back from what stopped it, then
        follow again; each time it blocks again before reaching the admin, the other side, one
        ROUND further out every second try (at most three)."""
        self.detours += 1
        (mx, mz), (ax, az) = xz(st["position"]), xz(admin["position"])
        length = math.hypot(ax - mx, az - mz) or 1.0
        ux, uz = (ax - mx) / length, (az - mz) / length  # toward the admin
        side = 1.0 if self.detours % 2 else -1.0
        reach = ROUND * min(3, (self.detours + 1) // 2)
        x, z = mx - uz * side * reach - ux, mz + ux * side * reach - uz
        ask("halt")
        ask("walk-to", f"{x:.2f}", f"{z:.2f}", "--wait", timeout=WALK_TIMEOUT)
        log("goes round", f"to {x:.1f}, {z:.1f}")
        self.follow(admin)

    def give_up(self, st, admin):
        """GIVE_UP_AFTER detours and blocked again: the admin is where it cannot get from here
        (fenced in, up a ledge). Stop trying, watch them, and try again once they move MOVED_ON."""
        ask("halt")
        self.waiting_for = xz(admin["position"])
        self.watched, self.sat, self.next_act = None, False, 0.0
        log("cannot get through: waits for the admin to move")
        self.watch(st, admin)

    def wait_below(self, st, admin):
        """Blocked under an admin up something: stop pressing at its foot, and watch them."""
        ask("halt")
        self.under, self.watched, self.sat, self.next_act = True, None, False, 0.0
        log("waits below the admin")
        self.watch(st, admin)

    def watch(self, st, admin):
        """Turn to the admin when they have moved round more than 40 degrees since the last look;
        after each look a cat sits, once (its chat line is the gesture)."""
        now = time.time()
        if now < self.next_act:
            return
        (mx, mz), (ax, az) = xz(st["position"]), xz(admin["position"])
        bearing = math.degrees(math.atan2(ax - mx, -(az - mz))) % 360
        if self.watched is None or abs((bearing - self.watched + 180) % 360 - 180) > 40:
            target = "@" + admin["handle"] if admin.get("handle") else admin["did"]
            if ask("face", target) is not None:
                self.watched, self.sat = bearing, False
                log("looks up at the admin")
        elif self.opts.persona == "cat" and not self.sat:
            self.sat = self.say_gesture("sit")
        self.next_act = now + random.uniform(*self.persona["every"])

    def yield_space(self, st, admin, others):
        """The factory steps aside when an avoided agent stands within GAP."""
        if self.opts.persona != "factory":
            return False
        me = xz(st["position"])
        if not any(math.dist(me, xz(o["position"])) < GAP for o in others):
            return False
        point = self.spot(admin, others, self.persona["ring"])
        if point:
            ask("walk-to", f"{point[0]:.2f}", f"{point[1]:.2f}")
            log("steps aside to", f"{point[0]:.1f}, {point[1]:.1f}")
            return True
        return False

    # ---- the loop -------------------------------------------------------------

    def step(self):
        seen = self.read()
        if seen is None:
            return
        st, admin, others = seen
        if admin is None:
            return
        self.track(admin)
        movement = st.get("movement") or {}
        busy = movement.get("doing") not in (None, "following")
        body = self.pick_body(st, admin)
        every = WATER_SWITCH_EVERY if FLEET["water"] in (body, self.body) else SWITCH_EVERY
        if body and self.wear(body, every):
            self.follow(admin)
            return
        if self.waiting_for is not None:
            if math.dist(xz(admin["position"]), self.waiting_for) <= MOVED_ON:
                self.watch(st, admin)
                return
            self.waiting_for, self.detours = None, 0
            log("the admin moved on: follows again")
            self.follow(admin)
            return
        if self.under:
            if self.up_above(st, admin, CLIMB - 1.0):
                self.watch(st, admin)
                return
            self.under = False
            log("the admin is down or away: follows again")
            self.follow(admin)
            return
        if self.blocked(st, movement):
            self.stall = None
            if not self.opts.fleet and self.up_above(st, admin, CLIMB):
                self.wait_below(st, admin)
            elif self.detours >= GIVE_UP_AFTER:
                self.give_up(st, admin)
            else:
                self.go_round(st, admin)
            return
        if self.idle_for() < self.opts.idle:
            if movement.get("doing") != "following":
                self.follow(admin)
            return
        if math.dist(xz(st["position"]), xz(admin["position"])) > NEAR:
            if movement.get("doing") != "following":
                self.follow(admin)
            return
        if busy or time.time() < self.next_act:
            return
        if not self.yield_space(st, admin, others):
            self.potter(st, admin, others)
        self.next_act = time.time() + random.uniform(*self.persona["every"])

    def run(self):
        try:
            items = (ask("inventory") or {}).get("items", [])
            worn = [i["name"] for i in items if i.get("worn") and i.get("is") == "avatar"]
            self.body = worn[0] if worn else None  # the whole avatar worn, not a hat on it
        except Exception as err:  # noqa: BLE001 - a body not known is one to find out by wearing
            log("could not read the inventory:", repr(err))
        log("companion", self.opts.persona, "side", self.opts.side, "wearing", self.body)
        while self.misses < 15:
            if self.opts.pause_file and os.path.exists(self.opts.pause_file):
                time.sleep(TICK)
                continue
            try:
                self.step()
            except Exception as err:  # noqa: BLE001 - it runs for hours: one bad answer must not end it
                log("skipped a tick:", repr(err))
            time.sleep(TICK)
        log("the agent is gone; stopping")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("persona", choices=sorted(PERSONAS))
    parser.add_argument("--side", choices=("left", "right"), default="right")
    parser.add_argument("--avoid", nargs="*", default=[], help="handles of agents to keep GAP from")
    parser.add_argument("--idle", type=float, default=8.0, help="seconds the admin stands before it potters")
    parser.add_argument("--fleet", action="store_true", help="a factory: change fleet bodies to suit the pace")
    parser.add_argument("--water-level", type=float, default=None,
                        help="the world's water level, m: a --fleet factory takes its boat into the water")
    parser.add_argument("--pause-file", default=None)
    opts = parser.parse_args()
    global PAUSE
    PAUSE = opts.pause_file
    Companion(opts).run()


if __name__ == "__main__":
    main()
