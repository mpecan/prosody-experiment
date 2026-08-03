// A SuperCollider receiver for prosody's OSC backend — `SPEC.md` §7.8.
//
// Without this the OSC path emits messages nobody listens for. Run it, then:
//
//     cargo run -p prosody-cli -- --send 127.0.0.1:57120 examples/blake-tyger.poem
//
// or, to recompile and replay on every save:
//
//     cargo run -p prosody-cli -- --watch --send 127.0.0.1:57120 examples/blake-tyger.poem
//
// Select everything and press Cmd+Enter (or Ctrl+Enter) to evaluate.
//
// What arrives, in order, is one `/prosody/tempo` message and then one `/prosody/note` per
// syllable:
//
//     /prosody/tempo  bpm
//     /prosody/note   beats layer voice gain note detune sharpness family sustain
//
// `beats` is when it sounds, counted from the start of the piece — the poem decides it and the
// tempo decides how long a beat is. `note` is -1 for an unpitched event, which is most of them:
// `SPEC.md` §4.5 gives a pitch to the word a line's rhyme rests on and to nothing else.

s.waitForBoot({
    var tempo = 120, start;

    // Two voices, matching `SPEC.md` §4.1's two layers. Which sounds these are is palette; that a
    // foundation is the pulse and a flourish ornaments it comes from the poem.
    SynthDef(\prosodyFoundation, { |out = 0, amp = 0.5, cutoff = 1200, attackTime = 0.01,
                                   sustainTime = 0.2, room = 0.1, crushAmount = 0|
        var env, sig;
        env = EnvGen.kr(Env.perc(attackTime, sustainTime), doneAction: 2);
        sig = SinOsc.ar(XLine.kr(180, 45, 0.06)) + (WhiteNoise.ar(0.2) * Env.perc(0.001, 0.02).kr);
        sig = LPF.ar(sig, cutoff.clip(80, 18000));
        // §4.3's morphological row: distance from the family's root erodes the sound rather than
        // replacing it, so "burn / burning / burnt" stay one instrument.
        sig = Decimator.ar(sig, 44100, (16 - (crushAmount * 12)).clip(4, 16));
        sig = sig * env * amp;
        Out.ar(out, Pan2.ar(FreeVerb.ar(sig, room.clip(0, 1) * 0.6), 0));
    }).add;

    SynthDef(\prosodyFlourish, { |out = 0, amp = 0.3, cutoff = 4000, attackTime = 0.005,
                                 sustainTime = 0.08, room = 0.2, crushAmount = 0|
        var env, sig;
        env = EnvGen.kr(Env.perc(attackTime, sustainTime), doneAction: 2);
        sig = HPF.ar(WhiteNoise.ar(0.5), 3000);
        sig = LPF.ar(sig, cutoff.clip(200, 18000));
        sig = Decimator.ar(sig, 44100, (16 - (crushAmount * 12)).clip(4, 16));
        sig = sig * env * amp;
        Out.ar(out, Pan2.ar(FreeVerb.ar(sig, room.clip(0, 1) * 0.6), 0.3));
    }).add;

    // §4.5: the melody is the rhyme words, so this plays only where a line rhymes.
    SynthDef(\prosodyPitch, { |out = 0, freq = 440, amp = 0.25, cutoff = 3000,
                              attackTime = 0.01, sustainTime = 0.4, room = 0.3|
        var env, sig;
        env = EnvGen.kr(Env.perc(attackTime, sustainTime), doneAction: 2);
        sig = VarSaw.ar(freq, 0, 0.3) + SinOsc.ar(freq * 2, 0, 0.15);
        sig = LPF.ar(sig, cutoff.clip(200, 18000));
        sig = sig * env * amp;
        Out.ar(out, Pan2.ar(FreeVerb.ar(sig, room.clip(0, 1) * 0.7), -0.2));
    }).add;

    s.sync;

    OSCdef(\prosodyTempo, { |msg|
        tempo = msg[1];
        // A fresh piece: everything after this is timed from now.
        start = Main.elapsedTime;
        ("prosody: tempo " ++ tempo ++ " bpm").postln;
    }, '/prosody/tempo');

    OSCdef(\prosodyNote, { |msg|
        var beats, layer, gain, note, detune, sharpness, family, sustain, when, seconds;
        # beats, layer, _voice, gain, note, detune, sharpness, family, sustain = msg[1..];

        seconds = 60 / tempo;
        // The whole schedule arrives at once and is *scheduled*, not played on arrival. The onset
        // is in the message precisely so a receiver can do this; sequencing in real time is this
        // end's job, not the compiler's.
        start = start ? Main.elapsedTime;
        when = start + (beats * seconds) - Main.elapsedTime;

        SystemClock.sched(max(when, 0), {
            if (note >= 0) {
                // §4.5: a scale degree became a MIDI note, and a slant rhyme sits off it.
                Synth(\prosodyPitch, [
                    \freq, (note + detune).midicps,
                    \amp, 0.25 * gain,
                    \cutoff, 400 + (sharpness * 4000),
                    \attackTime, 0.04 * (1 - sharpness),
                    \sustainTime, 0.4 * sustain,
                    \room, sharpness * 0.4
                ]);
            };
            Synth(
                if (layer == "foundation", \prosodyFoundation, \prosodyFlourish),
                [
                    \amp, gain * if (layer == "foundation", 0.5, 0.3),
                    // §4.4: distinctiveness drives brightness and attack.
                    \cutoff, 400 + (sharpness * 4000),
                    \attackTime, 0.04 * (1 - sharpness),
                    \sustainTime, 0.2 * sustain,
                    \room, sharpness * 0.4,
                    \crushAmount, family
                ]
            );
            nil
        });
    }, '/prosody/note');

    "prosody: listening on 57120 — send a poem with --send 127.0.0.1:57120".postln;
});
