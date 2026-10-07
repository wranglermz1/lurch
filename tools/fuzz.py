#!/usr/bin/env python3
"""Print a random drive script: tools/fuzz.py SEED [SECONDS]

Random chords of movement, jump, crouch, sprint and turning keys with random short
durations, so recordings cover combinations no hand-written scenario does. Jump and
crouch are mostly tapped; the seed makes a script reproducible.
"""
import random, sys

seed = int(sys.argv[1])
total = float(sys.argv[2]) if len(sys.argv) > 2 else 15.0
rng = random.Random(seed)
steps, t = [], 0.0
while t < total:
    keys = set()
    if rng.random() < 0.7:
        keys.add(rng.choice(['forward', 'forward', 'backward']))
    if rng.random() < 0.4:
        keys.add(rng.choice(['moveleft', 'moveright']))
    if rng.random() < 0.5:
        keys.add('speed')
    if rng.random() < 0.25:
        keys.add(rng.choice(['left', 'right']))
    if rng.random() < 0.15:
        keys.add('duck')
    d = rng.uniform(0.03, 0.4)
    if rng.random() < 0.2:
        steps.append(','.join(sorted(keys | {'jump'})) + ':0.04')
        t += 0.04
    if rng.random() < 0.08:
        steps.append(','.join(sorted(keys | {'duck'})) + f':{rng.uniform(0.2, 0.8):.2f}')
    steps.append((','.join(sorted(keys)) or '-') + f':{d:.2f}')
    t += d
steps.append('-:0.8')
print(' '.join(steps))
