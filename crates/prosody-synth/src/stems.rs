//! A block of rendered audio, one stereo stem per channel, and the duck (`docs/player.md` §4: the
//! render's output is stems, and the mix happens after, where mute and solo are a gain).

use crate::note::Channel;

/// How many stems: one per [`Channel`].
pub const STEMS: usize = Channel::ALL.len();

/// A block of stems, planar: each channel's left side, then its right, `frames` samples each, in
/// [`Channel::index`] order; then the duck, `frames` more — the music's level under the reading at
/// each frame, 1 for none. The page's worklet reads this layout from the transferred buffer.
///
/// The duck is written by the renderer because the reading's timeline is the renderer's
/// (`docs/player.md` §5, "The reading"): it comes from the same placements as the speech stem.
#[derive(Debug, Clone, PartialEq)]
pub struct Stems {
    frames: usize,
    data: Vec<f32>,
}

impl Stems {
    /// Numbers a block holds per frame: two a stem and the duck.
    pub const PER_FRAME: usize = 2 * STEMS + 1;

    /// A silent block `frames` long, the duck at 1.
    #[must_use]
    pub fn new(frames: usize) -> Self {
        let mut stems = Self {
            frames,
            data: vec![0.0; frames * Self::PER_FRAME],
        };
        stems.clear();
        stems
    }

    /// How many frames it holds.
    #[must_use]
    pub const fn frames(&self) -> usize {
        self.frames
    }

    /// Back to silence, the duck at 1, to be rendered again.
    pub fn clear(&mut self) {
        let stems = 2 * STEMS * self.frames;
        let split = stems.min(self.data.len());
        let (sound, duck) = self.data.split_at_mut(split);
        sound.fill(0.0);
        duck.fill(1.0);
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

    /// The duck: the music's level at each frame.
    #[must_use]
    pub fn duck(&self) -> &[f32] {
        let start = 2 * STEMS * self.frames;
        self.data.get(start..start + self.frames).unwrap_or(&[])
    }

    /// The duck, to write: what the reading's placements set.
    pub fn duck_mut(&mut self) -> &mut [f32] {
        let start = 2 * STEMS * self.frames;
        let end = start + self.frames;
        self.data.get_mut(start..end).unwrap_or(&mut [])
    }

    /// Both sides of a stem, whole.
    pub(crate) fn sides_mut(&mut self, channel: Channel) -> Option<(&mut [f32], &mut [f32])> {
        let frames = self.frames;
        let stem = self
            .data
            .get_mut(channel.index() * 2 * frames..(channel.index() + 1) * 2 * frames)?;
        Some(stem.split_at_mut(frames))
    }
}

#[cfg(test)]
mod tests {
    use super::{STEMS, Stems};
    use crate::note::Channel;

    #[test]
    fn each_stem_is_left_then_right_in_channel_order_then_the_duck() {
        let mut stems = Stems::new(4);
        let (left, right) = stems.sides_mut(Channel::Melody).unwrap();
        left[1..3].fill(1.0);
        right[1..3].fill(2.0);
        // Melody is stem 2: its left side is samples 16..20, its right 20..24; the duck follows
        // all five stems, at 40..44, and is 1.
        let all = stems.as_slice();
        assert_eq!(all.len(), 4 * (2 * STEMS + 1));
        assert_eq!(&all[16..20], &[0.0, 1.0, 1.0, 0.0]);
        assert_eq!(&all[20..24], &[0.0, 2.0, 2.0, 0.0]);
        assert_eq!(&all[40..44], &[1.0; 4]);
        assert_eq!(stems.side(Channel::Melody, true), &[0.0, 2.0, 2.0, 0.0]);
        assert_eq!(stems.duck(), &[1.0; 4]);
        stems.clear();
        assert!(stems.side(Channel::Melody, false).iter().all(|x| *x == 0.0));
        assert_eq!(stems.duck(), &[1.0; 4]);
    }
}
