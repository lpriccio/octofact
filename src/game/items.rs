use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemId {
    #[default]
    NullSet,
    Point,
    Preimage,
    Wavelet,
    LineSegment,
    ExactSequence,
    Identity,
    Square,
    Cube,
    StandingWave,
    Function,
    NeckerCube,
    Image,
    Belt,
    AxiomaticScience,
    Composer,
    Inverter,
    Embedder,
    Quotient,
    Transformer,
    KnowledgeSheaf,
    Quadrupole,
    Dynamo,
    RootOfUnity,
    Kernel,
    Quantum,
    Splitter,
    SourceMachine,
}

impl ItemId {
    pub fn all() -> &'static [ItemId] {
        use ItemId::*;
        &[
            NullSet, Point, Preimage, Wavelet, LineSegment, ExactSequence,
            Identity, Square, Cube, StandingWave, Function, NeckerCube,
            Image, Belt, AxiomaticScience, Composer, Inverter, Embedder,
            Quotient, Transformer, KnowledgeSheaf, Quadrupole, Dynamo,
            RootOfUnity, Kernel, Quantum, Splitter, SourceMachine,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::NullSet => "Null Set",
            Self::Point => "Point",
            Self::Preimage => "Preimage",
            Self::Wavelet => "Wavelet",
            Self::LineSegment => "Line Segment",
            Self::ExactSequence => "Exact Sequence",
            Self::Identity => "Identity",
            Self::Square => "Square",
            Self::Cube => "Cube",
            Self::StandingWave => "Standing Wave",
            Self::Function => "Function",
            Self::NeckerCube => "Necker Cube",
            Self::Image => "Image",
            Self::Belt => "Belt",
            Self::AxiomaticScience => "Axiomatic Science",
            Self::Composer => "Composer",
            Self::Inverter => "Inverter",
            Self::Embedder => "Embedder",
            Self::Quotient => "Quotient",
            Self::Transformer => "Transformer",
            Self::KnowledgeSheaf => "Knowledge Sheaf",
            Self::Quadrupole => "Quadrupole",
            Self::Dynamo => "Dynamo",
            Self::RootOfUnity => "Root of Unity",
            Self::Kernel => "Kernel",
            Self::Quantum => "Quantum",
            Self::Splitter => "Splitter",
            Self::SourceMachine => "Source",
        }
    }

    pub fn category(&self) -> ItemCategory {
        match self {
            Self::NullSet | Self::Point | Self::Preimage | Self::Wavelet => {
                ItemCategory::RawResource
            }
            Self::LineSegment | Self::ExactSequence | Self::Identity
            | Self::Square | Self::Cube | Self::StandingWave
            | Self::Function | Self::NeckerCube | Self::Image
            | Self::AxiomaticScience => ItemCategory::Intermediate,
            Self::Belt | Self::Quadrupole | Self::Dynamo | Self::Splitter => ItemCategory::Infrastructure,
            Self::Composer | Self::Inverter | Self::Embedder
            | Self::Quotient | Self::Transformer | Self::KnowledgeSheaf
            | Self::SourceMachine => {
                ItemCategory::Machine
            }
            Self::RootOfUnity | Self::Kernel | Self::Quantum => ItemCategory::Advanced,
        }
    }

    pub fn tier(&self) -> u32 {
        match self {
            Self::NullSet | Self::Point | Self::Preimage | Self::Wavelet => 0,
            Self::RootOfUnity | Self::Kernel | Self::Quantum
            | Self::Embedder | Self::Quotient | Self::Transformer => 2,
            Self::SourceMachine | Self::Splitter => 0,
            _ => 1,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::NullSet => "Crystallized absence. The Surface produces it where the metric thins toward zero.",
            Self::Point => "Dimensionless position-object. The atomic unit of geometry on the Surface.",
            Self::Preimage => "Ghost of a function that hasn't been applied yet. Shimmering, unstable.",
            Self::Wavelet => "Oscillatory fragment of the Surface's vibrational modes.",
            Self::LineSegment => "Two points, connected. The simplest structure.",
            Self::ExactSequence => "Three preimages composed into algebraic certainty.",
            Self::Identity => "A null set made useful. The do-nothing that does everything.",
            Self::Square => "Four line segments. A flat patch of order on the curved Surface.",
            Self::Cube => "Six squares folded into three dimensions. Impossibly stable.",
            Self::StandingWave => "Two wavelets in resonance. The Surface hums through it.",
            Self::Function => "A preimage realized. The mapping made concrete.",
            Self::NeckerCube => "A cube inverted. It flickers between two interpretations.",
            Self::Image => "A preimage inverted. What was latent is now manifest.",
            Self::Belt => "Manufactured logistics. Carries items along grid edges.",
            Self::AxiomaticScience => "Research feedstock. Distilled from cubes into pure axioms.",
            Self::Composer => "The machine that combines. Interlocking rotating rings.",
            Self::Inverter => "The machine that reverses. A mirrored prism.",
            Self::Embedder => "Maps one object into the structure of another. T2 machine.",
            Self::Quotient => "Divides one structure by another. Produces quotient and remainder.",
            Self::Transformer => "Applies transformation across multiple inputs simultaneously.",
            Self::KnowledgeSheaf => "Science consumer. Pages fan through proofs when active.",
            Self::Quadrupole => "Electrical pole. Transmits power across the grid.",
            Self::Dynamo => "Power generator. Two quadrupoles in harness.",
            Self::RootOfUnity => "A preimage embedded in unity. Cycles back to itself.",
            Self::Kernel => "An identity embedded in a preimage. The null space made real.",
            Self::Quantum => "A standing wave embedded in a cube. Probability crystallized.",
            Self::Splitter => "Universal junction. Merges, splits, or balances item flows depending on belt connections.",
            Self::SourceMachine => "Debug machine. Produces any item from nothing.",
        }
    }

    pub fn icon_params(&self) -> IconParams {
        match self {
            // Raw resources — circles
            Self::NullSet => IconParams {
                shape: IconShape::Circle,
                primary_color: [1.0, 0.9, 0.2],
                secondary_color: [0.8, 0.7, 0.1],
            },
            Self::Point => IconParams {
                shape: IconShape::Circle,
                primary_color: [0.9, 0.9, 1.0],
                secondary_color: [0.6, 0.6, 0.8],
            },
            Self::Preimage => IconParams {
                shape: IconShape::Circle,
                primary_color: [0.5, 0.3, 0.8],
                secondary_color: [0.7, 0.5, 1.0],
            },
            Self::Wavelet => IconParams {
                shape: IconShape::Circle,
                primary_color: [0.2, 0.6, 0.8],
                secondary_color: [0.4, 0.8, 1.0],
            },
            // T1 intermediates — triangles and squares
            Self::LineSegment => IconParams {
                shape: IconShape::Square,
                primary_color: [0.8, 0.8, 0.7],
                secondary_color: [0.5, 0.5, 0.4],
            },
            Self::ExactSequence => IconParams {
                shape: IconShape::Triangle,
                primary_color: [0.6, 0.3, 0.7],
                secondary_color: [0.4, 0.2, 0.5],
            },
            Self::Identity => IconParams {
                shape: IconShape::Square,
                primary_color: [0.7, 0.7, 0.9],
                secondary_color: [0.4, 0.4, 0.6],
            },
            Self::Square => IconParams {
                shape: IconShape::Square,
                primary_color: [0.8, 0.7, 0.5],
                secondary_color: [0.6, 0.5, 0.3],
            },
            Self::Cube => IconParams {
                shape: IconShape::Hexagon,
                primary_color: [0.7, 0.6, 0.4],
                secondary_color: [0.5, 0.4, 0.2],
            },
            Self::StandingWave => IconParams {
                shape: IconShape::Triangle,
                primary_color: [0.3, 0.7, 0.9],
                secondary_color: [0.2, 0.5, 0.7],
            },
            Self::Function => IconParams {
                shape: IconShape::Triangle,
                primary_color: [0.8, 0.4, 0.6],
                secondary_color: [0.6, 0.2, 0.4],
            },
            Self::NeckerCube => IconParams {
                shape: IconShape::Hexagon,
                primary_color: [0.6, 0.5, 0.7],
                secondary_color: [0.4, 0.3, 0.5],
            },
            Self::Image => IconParams {
                shape: IconShape::Triangle,
                primary_color: [0.7, 0.5, 0.9],
                secondary_color: [0.5, 0.3, 0.7],
            },
            Self::AxiomaticScience => IconParams {
                shape: IconShape::Hexagon,
                primary_color: [0.9, 0.8, 0.3],
                secondary_color: [0.7, 0.6, 0.1],
            },
            // Infrastructure — octagons
            Self::Belt => IconParams {
                shape: IconShape::Octagon,
                primary_color: [0.6, 0.6, 0.6],
                secondary_color: [0.3, 0.3, 0.3],
            },
            Self::Quadrupole => IconParams {
                shape: IconShape::Octagon,
                primary_color: [0.9, 0.8, 0.2],
                secondary_color: [0.7, 0.6, 0.1],
            },
            Self::Dynamo => IconParams {
                shape: IconShape::Octagon,
                primary_color: [1.0, 0.9, 0.3],
                secondary_color: [0.8, 0.7, 0.1],
            },
            Self::Splitter => IconParams {
                shape: IconShape::Octagon,
                primary_color: [0.3, 0.8, 0.7],
                secondary_color: [0.1, 0.6, 0.5],
            },
            // Machines — diamonds
            Self::Composer => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.4, 0.6, 0.8],
                secondary_color: [0.2, 0.4, 0.6],
            },
            Self::Inverter => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.8, 0.4, 0.4],
                secondary_color: [0.6, 0.2, 0.2],
            },
            Self::Embedder => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.5, 0.8, 0.5],
                secondary_color: [0.3, 0.6, 0.3],
            },
            Self::Quotient => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.7, 0.5, 0.3],
                secondary_color: [0.5, 0.3, 0.1],
            },
            Self::Transformer => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.6, 0.3, 0.8],
                secondary_color: [0.4, 0.1, 0.6],
            },
            Self::KnowledgeSheaf => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.9, 0.7, 0.2],
                secondary_color: [0.7, 0.5, 0.1],
            },
            // T2 advanced — stars
            Self::RootOfUnity => IconParams {
                shape: IconShape::Star,
                primary_color: [0.9, 0.3, 0.9],
                secondary_color: [0.6, 0.1, 0.6],
            },
            Self::Kernel => IconParams {
                shape: IconShape::Star,
                primary_color: [0.3, 0.9, 0.6],
                secondary_color: [0.1, 0.6, 0.4],
            },
            Self::Quantum => IconParams {
                shape: IconShape::Star,
                primary_color: [0.3, 0.6, 1.0],
                secondary_color: [0.1, 0.3, 0.7],
            },
            // Debug source — bright green diamond
            Self::SourceMachine => IconParams {
                shape: IconShape::Diamond,
                primary_color: [0.2, 1.0, 0.2],
                secondary_color: [0.1, 0.7, 0.1],
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemCategory {
    RawResource,
    Intermediate,
    Infrastructure,
    Machine,
    Advanced,
}

impl ItemCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::RawResource => "Raw Resources",
            Self::Intermediate => "Intermediates",
            Self::Infrastructure => "Infrastructure",
            Self::Machine => "Machines",
            Self::Advanced => "Advanced",
        }
    }

    pub fn all() -> &'static [ItemCategory] {
        &[
            ItemCategory::RawResource,
            ItemCategory::Intermediate,
            ItemCategory::Infrastructure,
            ItemCategory::Machine,
            ItemCategory::Advanced,
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IconParams {
    pub shape: IconShape,
    pub primary_color: [f32; 3],
    pub secondary_color: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconShape {
    Circle,
    Triangle,
    Square,
    Hexagon,
    Diamond,
    Octagon,
    Star,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MachineType {
    Composer,
    Inverter,
    Embedder,
    Quotient,
    Transformer,
    Source,
}

impl MachineType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Composer => "Composer",
            Self::Inverter => "Inverter",
            Self::Embedder => "Embedder",
            Self::Quotient => "Quotient",
            Self::Transformer => "Transformer",
            Self::Source => "Source",
        }
    }

    /// Footprint in grid cells: (width, height). Matches shader `machine_size()`.
    /// Width = East-West extent, Height = North-South extent (facing North).
    pub fn footprint(&self) -> (i32, i32) {
        match self {
            Self::Source => (1, 1),
            Self::Composer => (2, 2),
            Self::Inverter | Self::Embedder | Self::Quotient | Self::Transformer => (3, 3),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Recipe {
    pub machine: MachineType,
    pub inputs: Vec<(ItemId, u32)>,
    pub output: ItemId,
    #[allow(dead_code)] // validated in tests; will be read by machine simulation
    pub output_count: u32,
}

pub fn all_recipes() -> Vec<Recipe> {
    use ItemId::*;
    let c = MachineType::Composer;
    let i = MachineType::Inverter;
    let e = MachineType::Embedder;
    vec![
        // T1 Composition
        Recipe { machine: c, inputs: vec![(Point, 2)], output: LineSegment, output_count: 1 },
        Recipe { machine: c, inputs: vec![(Preimage, 3)], output: ExactSequence, output_count: 1 },
        Recipe { machine: c, inputs: vec![(NullSet, 1)], output: Identity, output_count: 1 },
        Recipe { machine: c, inputs: vec![(LineSegment, 4)], output: Square, output_count: 1 },
        Recipe { machine: c, inputs: vec![(Square, 6)], output: Cube, output_count: 1 },
        Recipe { machine: c, inputs: vec![(Wavelet, 2)], output: StandingWave, output_count: 1 },
        // T1 Inversion
        Recipe { machine: i, inputs: vec![(Preimage, 1)], output: Function, output_count: 1 },
        Recipe { machine: i, inputs: vec![(Cube, 1)], output: NeckerCube, output_count: 1 },
        Recipe { machine: i, inputs: vec![(Preimage, 1)], output: Image, output_count: 1 },
        // Self-bootstrapping
        Recipe { machine: c, inputs: vec![(LineSegment, 1)], output: Belt, output_count: 1 },
        Recipe { machine: c, inputs: vec![(Cube, 1)], output: AxiomaticScience, output_count: 1 },
        Recipe { machine: c, inputs: vec![(Function, 2)], output: Composer, output_count: 1 },
        Recipe { machine: i, inputs: vec![(Composer, 1)], output: Inverter, output_count: 1 },
        Recipe { machine: c, inputs: vec![(AxiomaticScience, 12)], output: KnowledgeSheaf, output_count: 1 },
        // Power chain
        Recipe { machine: c, inputs: vec![(Identity, 4)], output: Quadrupole, output_count: 1 },
        Recipe { machine: c, inputs: vec![(Quadrupole, 2)], output: Dynamo, output_count: 1 },
        // T2 Embedding
        Recipe { machine: e, inputs: vec![(Preimage, 1), (Identity, 1)], output: RootOfUnity, output_count: 1 },
        Recipe { machine: e, inputs: vec![(Identity, 1), (Preimage, 1)], output: Kernel, output_count: 1 },
        Recipe { machine: e, inputs: vec![(StandingWave, 1), (Cube, 1)], output: Quantum, output_count: 1 },
        // Source machine: one recipe per item (no inputs required)
        Recipe { machine: MachineType::Source, inputs: vec![], output: NullSet, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Point, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Preimage, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Wavelet, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: LineSegment, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: ExactSequence, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Identity, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Square, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Cube, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: StandingWave, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Function, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: NeckerCube, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Image, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Belt, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: AxiomaticScience, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: RootOfUnity, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Kernel, output_count: 1 },
        Recipe { machine: MachineType::Source, inputs: vec![], output: Quantum, output_count: 1 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_items_have_display_names() {
        for item in ItemId::all() {
            assert!(!item.display_name().is_empty(), "{:?} has empty display name", item);
        }
    }

    #[test]
    fn test_all_items_have_categories() {
        for item in ItemId::all() {
            let _ = item.category();
        }
    }

    #[test]
    fn test_all_items_have_descriptions() {
        for item in ItemId::all() {
            assert!(!item.description().is_empty(), "{:?} has empty description", item);
        }
    }

    #[test]
    fn test_all_items_have_icon_params() {
        for item in ItemId::all() {
            let params = item.icon_params();
            // Colors should be in [0,1] range
            for &c in &params.primary_color {
                assert!((0.0..=1.0).contains(&c), "{:?} primary color out of range", item);
            }
            for &c in &params.secondary_color {
                assert!((0.0..=1.0).contains(&c), "{:?} secondary color out of range", item);
            }
        }
    }

    #[test]
    fn test_all_items_count() {
        assert_eq!(ItemId::all().len(), 28);
    }

    #[test]
    fn test_all_recipes_reference_valid_items() {
        let all_items: std::collections::HashSet<ItemId> = ItemId::all().iter().copied().collect();
        for recipe in all_recipes() {
            for (item, count) in &recipe.inputs {
                assert!(all_items.contains(item), "Recipe for {:?} references unknown input {:?}", recipe.output, item);
                assert!(*count > 0, "Recipe for {:?} has zero count input", recipe.output);
            }
            assert!(all_items.contains(&recipe.output), "Recipe output {:?} is unknown", recipe.output);
            assert!(recipe.output_count > 0);
        }
    }
}
