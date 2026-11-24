use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use mesh_core_elements::{NodeId, style::Color};
use skia_safe::{
    ImageFilter, Matrix, Picture, Rect, RuntimeEffect, SamplingOptions, image_filters,
    runtime_effect::RuntimeShaderBuilder,
};

use crate::display_list::DisplayMaterial;

use super::ClipRect;

const MAX_SOURCE_BYTES: usize = 2048;
const MAX_LAYER_PIXELS: i64 = 4 * 1024 * 1024;
const MATERIAL_CACHE_ENTRIES: usize = 32;

#[derive(Debug, Clone)]
pub(crate) struct NamedRenderPicture {
    pub(crate) picture: Picture,
    pub(crate) bounds: Rect,
}

impl PartialEq for NamedRenderPicture {
    fn eq(&self, other: &Self) -> bool {
        self.picture.unique_id() == other.picture.unique_id() && self.bounds == other.bounds
    }
}

type CompiledMaterial = Result<RuntimeEffect, String>;

thread_local! {
    static MATERIALS: RefCell<VecDeque<(Arc<str>, CompiledMaterial)>> =
        const { RefCell::new(VecDeque::new()) };
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PainterMaterial {
    pub(crate) shader: DisplayMaterial,
    pub(crate) node_id: NodeId,
    pub(crate) bounds: ClipRect,
    pub(crate) tint: Color,
    pub(crate) named_source: Option<NamedRenderPicture>,
}

impl PainterMaterial {
    pub(crate) fn image_filter(
        &self,
        crop: &Rect,
        input: Option<ImageFilter>,
        within_depth: bool,
    ) -> Result<ImageFilter, String> {
        if !within_depth {
            return Err("Material nesting exceeds the offscreen layer limit".into());
        }
        let width = i64::from(self.bounds.width);
        let height = i64::from(self.bounds.height);
        if width <= 0
            || height <= 0
            || width * height > MAX_LAYER_PIXELS
            || crop.width() * crop.height() > MAX_LAYER_PIXELS as f32
        {
            return Err("Material box exceeds the offscreen pixel limit".into());
        }
        let effect = compiled_material(&self.shader.source)?;
        let mut builder = RuntimeShaderBuilder::new(effect);
        let tint = [self.tint.r, self.tint.g, self.tint.b, self.tint.a]
            .map(|value| f32::from(value) / 255.0);
        for (name, data) in [
            ("time", &[self.shader.time][..]),
            ("values", &self.shader.values),
            ("tint", &tint),
            (
                "bounds",
                &[
                    self.bounds.x as f32,
                    self.bounds.y as f32,
                    width as f32,
                    height as f32,
                ],
            ),
        ] {
            builder
                .set_uniform_float(name, data)
                .map_err(|error| error.to_string())?;
        }
        let input = match &self.named_source {
            Some(source) => {
                let destination = Rect::from_xywh(
                    self.bounds.x as f32, self.bounds.y as f32, width as f32, height as f32,
                );
                let transform = Matrix::rect_2_rect(source.bounds, destination, None)
                    .ok_or_else(|| "Unable to place the material source in its layer".to_string())?;
                let picture = image_filters::picture(source.picture.clone(), &source.bounds)
                    .ok_or_else(|| "Unable to read the material source picture".to_string())?;
                Some(image_filters::matrix_transform(&transform, SamplingOptions::default(), picture)
                    .ok_or_else(|| "Unable to transform the material source picture".to_string())?)
            }
            None if self.shader.input_source.is_some() => {
                return Err("Material source is missing, ambiguous, or outside its limits".into());
            }
            None => input,
        };
        let input = image_filters::crop(crop, None, input);
        let filtered = image_filters::runtime_shader(&builder, "source", input)
            .ok_or_else(|| "Unable to create the material's subtree filter".to_string())?;
        image_filters::crop(crop, None, filtered)
            .ok_or_else(|| "Unable to clip the material to its layer".into())
    }
}

fn compiled_material(source: &Arc<str>) -> CompiledMaterial {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "Material programs are limited to {MAX_SOURCE_BYTES} bytes"
        ));
    }
    MATERIALS.with_borrow_mut(|cache| {
        if let Some(index) = cache.iter().position(|(cached, _)| cached == source) {
            let entry = cache.remove(index).expect("cached material index");
            let effect = entry.1.clone();
            cache.push_back(entry);
            return effect;
        }
        let effect = compile(source);
        if cache.len() >= MATERIAL_CACHE_ENTRIES {
            cache.pop_front();
        }
        cache.push_back((Arc::clone(source), effect.clone()));
        effect
    })
}

fn compile(source: &str) -> CompiledMaterial {
    if source.bytes().any(|byte| {
        !(byte.is_ascii_graphic() || byte.is_ascii_whitespace())
            || matches!(byte, b'{' | b'}' | b'[' | b']' | b'#' | b'\\')
    }) || source.bytes().filter(|&byte| byte == b'(').count() > 64
        || source.bytes().filter(|&byte| byte == b';').count() > 32
        || source
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .any(|word| matches!(word, "for" | "while" | "do" | "switch" | "main" | "discard"))
    {
        return Err("Material must be a bounded, loop-free SkSL fragment body".into());
    }
    let program = format!(
        "uniform shader source;\n\
         uniform float time;\n\
         uniform float4 values;\n\
         uniform float4 tint;\n\
         uniform float4 bounds;\n\
         half4 main(float2 p) {{\n{source}\n}}",
    );
    RuntimeEffect::make_for_shader(&program, None).map_err(|error| {
        let message = error
            .lines()
            .find(|line| line.starts_with("error:"))
            .unwrap_or(&error);
        format!(
            "Material compilation failed: {}",
            message.chars().take(200).collect::<String>()
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::{FrontendRenderEngine, PixelBuffer};
    use mesh_core_elements::{LayoutRect, Prop, WidgetNode, style::Color};

    fn material_tree(program: &str) -> WidgetNode {
        let mut root = WidgetNode::new("box");
        root.id = 1;
        root.layout = LayoutRect {
            x: 3.0,
            y: 2.0,
            width: 20.0,
            height: 12.0,
        };
        root.attributes.insert(Prop::Material, program.into());
        root.attributes.insert(Prop::MaterialTime, "0.5".into());
        root.attributes
            .insert(Prop::MaterialValues, "0.2 0 0 0".into());
        root.computed_style.color = Color::from_hex("#00ff00").unwrap();
        let mut child = WidgetNode::new("box");
        child.id = 2;
        child.layout = LayoutRect {
            x: 7.0,
            y: 4.0,
            width: 12.0,
            height: 8.0,
        };
        child.computed_style.background_color = Color::from_hex("#ff0000").unwrap();
        root.children.push(child);
        root
    }

    fn pixel(buffer: &PixelBuffer, x: usize, y: usize) -> [u8; 4] {
        let offset = (y * buffer.width() as usize + x) * 4;
        buffer.data()[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn material_samples_only_its_subtree_and_accepts_bound_uniforms() {
        let root = material_tree(
            "half4 c = source.eval(p);\n\
             return half4(c.r * half(values.x), c.a * half(tint.g * time), 0, c.a);",
        );
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(32, 24);
        engine.render_tree(&root, &mut buffer, 1.0);
        assert!(
            engine.painter_diagnostics().is_empty(),
            "{:?}",
            engine.painter_diagnostics()
        );
        let tinted = pixel(&buffer, 10, 8);
        assert_eq!(tinted[0], 0);
        assert!((126..=129).contains(&tinted[1]), "{tinted:?}");
        assert!((50..=52).contains(&tinted[2]), "{tinted:?}");
        assert_eq!(tinted[3], 255);
        assert_eq!(pixel(&buffer, 30, 8), [0; 4]);
    }

    #[test]
    fn rejected_material_keeps_ordinary_pixels_and_names_the_source_node() {
        let root = material_tree("while (true) {} return half4(0);");
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(32, 24);
        engine.render_tree(&root, &mut buffer, 1.0);
        assert_eq!(pixel(&buffer, 10, 8), [0, 0, 255, 255]);
        assert!(engine.painter_diagnostics().iter().any(|diagnostic| {
            diagnostic.source.as_ref().is_some_and(|source| {
                source.node_id == Some(1) && source.property.as_deref() == Some("material")
            })
        }));
    }

    #[test]
    fn nested_materials_read_the_result_of_the_inner_pass() {
        let mut root = material_tree("half4 c = source.eval(p); return half4(0, c.b, 0, c.a);");
        root.children[0].attributes.insert(
            Prop::Material,
            "half4 c = source.eval(p); return half4(0, 0, c.r, c.a);".into(),
        );
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(32, 24);
        engine.render_tree(&root, &mut buffer, 1.0);
        assert_eq!(pixel(&buffer, 10, 8), [0, 255, 0, 255]);
        assert!(engine.painter_diagnostics().is_empty());
    }

    #[test]
    fn a_procedural_material_can_fill_a_transparent_box() {
        let mut root = material_tree("return half4(0, 1, 0, 1);");
        root.children.clear();
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(32, 24);
        engine.render_tree(&root, &mut buffer, 1.0);
        assert_eq!(pixel(&buffer, 10, 8), [0, 255, 0, 255]);
        assert_eq!(pixel(&buffer, 30, 8), [0; 4]);
    }

    #[test]
    fn material_bounds_and_sampling_share_paint_coordinates() {
        let root = material_tree(
            "if (p.x < bounds.x + bounds.z / 2.0) return source.eval(p + float2(4, 0));\n\
             return half4(0);",
        );
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(32, 24);
        engine.render_tree(&root, &mut buffer, 1.0);
        assert_eq!(pixel(&buffer, 4, 8), [0, 0, 255, 255]);
        assert_eq!(pixel(&buffer, 14, 8), [0; 4]);
    }

    #[test]
    fn named_material_source_is_sized_to_the_consumer_and_updates_disjoint_damage() {
        use crate::display_list::{DisplayListRepaintPolicy, RetainedDisplayList};

        let mut root = WidgetNode::new("box");
        root.id = 1;
        root.layout.width = 60.0;
        root.layout.height = 22.0;
        let mut consumer = WidgetNode::new("box");
        consumer.id = 2;
        consumer.set_render_source_scope("test".into());
        consumer.layout = LayoutRect { x: 30.0, y: 4.0, width: 20.0, height: 12.0 };
        consumer.attributes.insert(Prop::Material, "return source.eval(p);".into());
        consumer.attributes.insert(Prop::MaterialSource, "preview".into());
        consumer.computed_style.background_color = Color::from_hex("#00ff00").unwrap();
        let mut producer = WidgetNode::new("box");
        producer.id = 3;
        producer.set_render_source_scope("test".into());
        producer.layout = LayoutRect { x: 2.0, y: 4.0, width: 10.0, height: 8.0 };
        producer.attributes.insert(Prop::RenderSource, "preview".into());
        producer.computed_style.background_color = Color::from_hex("#ff0000").unwrap();
        root.children = vec![consumer, producer].into();

        let engine = FrontendRenderEngine::new();
        let mut list = RetainedDisplayList::default();
        let render = |list: &RetainedDisplayList| {
            let mut buffer = PixelBuffer::new(60, 22);
            let commands = list.select_paint_commands_for_rects(
                list.damage_rects(), DisplayListRepaintPolicy::MinimalDamage,
            );
            engine.render_selected_display_list_for_module(
                &commands, &mut buffer, 1.0, None, None, None,
            );
            buffer
        };
        list.update(&root, 60, 22, false, true);
        assert_eq!(pixel(&render(&list), 47, 13), [0, 0, 255, 255]);
        list.mark_painted();
        root.children[1].computed_style.background_color = Color::from_hex("#0000ff").unwrap();
        list.update(&root, 60, 22, false, true);
        assert!(list.damage_rects().iter().any(|rect| rect.x + rect.width >= 50));
        assert_eq!(pixel(&render(&list), 47, 13), [255, 0, 0, 255]);
        assert!(engine.painter_diagnostics().is_empty());

        list.mark_painted();
        root.children[1].set_render_source_scope("different-instance".into());
        list.update(&root, 60, 22, false, true);
        assert_eq!(pixel(&render(&list), 47, 13), [0, 255, 0, 255]);
        root.children[1].set_render_source_scope("test".into());

        // Renaming a source changes no producer pixels, but must retire its
        // old binding and repaint the consumer with its unmodified fill.
        list.mark_painted();
        root.children[1].attributes.insert(Prop::RenderSource, "renamed".into());
        list.update(&root, 60, 22, false, true);
        assert_eq!(pixel(&render(&list), 47, 13), [0, 255, 0, 255]);
        assert!(!engine.painter_diagnostics().is_empty());

        list.mark_painted();
        root.children[0].attributes.insert(Prop::RenderSource, "preview".into());
        list.update(&root, 60, 22, false, true);
        assert_eq!(pixel(&render(&list), 47, 13), [0, 255, 0, 255]);
    }
}
