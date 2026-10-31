//! A block of rendered audio, one stereo stem per channel (`docs/player.md` §4: the render's output
//! is stems, and the mix happens after, where mute and solo are a gain).

use crate::note::Channel;

/// How many stems: one per [`Channel`].
pub const STEMS: usize = Channel::ALL.len();

/// A block of stems, planar: each channel's left side, then its right, `frames` samples each, in
/// [`Channel::index`] order. The page's worklet reads this layout from the transferred buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct Stems {
    frames: usize,
    data: Vec<f32>,
}

impl Stems {
    /// A silent block `frames` long.
    #[must_use]
    pub fn new(frames: usize) -> Self {
        Self {
            frames,
            data: vec![0.0; frames * 2 * STEMS],
        }
    }

    /// How many frames it holds.
    #[must_use]
    pub const fn frames(&self) -> usize {
        self.frames
    }

    /// Back to silence, to be rendered again.
    pub fn clear(&mut self) {
        self.data.fill(0.0);
    }

    /// Every sample, in the planar layout.
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// One side of one stem.
    #[must_use]
    pub fn side(&self, channel: Channel, right: bool) -> &[f32] {
        let start = (channel.index() * 2 + usize::from(right)) * self.frames;
        self.data.get(start..start + self.frames).unwrap_or(&[])
    }

    /// Both sides of a stem, from frame `from`, `length` frames: where a voice adds itself.
    pub(crate) fn sides_mut(
        &mut self,
        channel: Channel,
        from: usize,
        length: usize,
    ) -> Option<(&mut [f32], &mut [f32])> {
        let frames = self.frames;
        let stem = self
            .data
            .get_mut(channel.index() * 2 * frames..(channel.index() + 1) * 2 * frames)?;
        let (left, right) = stem.split_at_mut(frames);
        Some((
            left.get_mut(from..from + length)?,
            right.get_mut(from..from + length)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::Stems;
    use crate::note::Channel;

    #[test]
    fn each_stem_is_left_then_right_in_channel_order() {
        let mut stems = Stems::new(4);
        let (left, right) = stems.sides_mut(Channel::Melody, 1, 2).unwrap();
        left.fill(1.0);
        right.fill(2.0);
        // Melody is stem 2: its left side is samples 16..20, its right 20..24.
        let all = stems.as_slice();
        assert_eq!(&all[16..20], &[0.0, 1.0, 1.0, 0.0]);
        assert_eq!(&all[20..24], &[0.0, 2.0, 2.0, 0.0]);
        assert_eq!(stems.side(Channel::Melody, true), &[0.0, 2.0, 2.0, 0.0]);
        assert!(stems.sides_mut(Channel::Drums, 3, 2).is_none());
        stems.clear();
        assert!(stems.as_slice().iter().all(|x| *x == 0.0));
    }
}
