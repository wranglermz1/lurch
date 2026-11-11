package lurch.mc;

import java.util.List;
import java.util.Locale;

import net.minecraft.client.gui.components.AbstractSliderButton;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import net.minecraft.util.Mth;

/**
 * lurch's options: the Apex FOV, autosprint, lurch's own mouse sensitivity, the legend and the
 * heirloom, and the HUD's layout. A change is kept (`LurchConfig`) and sent to lurch-mc at once.
 */
public final class LurchOptionsScreen extends Screen {
	/** Apex's FOV slider runs 70 to 110 over `cl_fovScale` 1.0 to 1.55. */
	private static final int FOV_MIN = 70, FOV_MAX = 110;
	private static final float SCALE_PER_DEGREE = 0.55f / (FOV_MAX - FOV_MIN);
	private static final double SENS_MIN = 0.1, SENS_MAX = 20;

	private final Screen parent;

	public LurchOptionsScreen(Screen parent) {
		super(Component.literal("lurch options"));
		this.parent = parent;
	}

	private static String value(String name, String fallback) {
		String v = LurchConfig.option(name);
		if (v == null) {
			v = Lurch.hostOption(name);
		}
		return v == null ? fallback : v;
	}

	private static void set(String name, String v) {
		LurchConfig.setOption(name, v);
		Lurch.sendOption(name, v);
	}

	private static float parse(String v, float fallback) {
		try {
			return Float.parseFloat(v);
		} catch (NumberFormatException e) {
			return fallback;
		}
	}

	@Override
	protected void init() {
		int x = width / 2 - 155, y = height / 6, w = 150, row = 24;
		float scale = parse(value("fov_scale", "1.27216"), 1.27216f);
		int fov = Mth.clamp(Math.round(FOV_MIN + (scale - 1f) / SCALE_PER_DEGREE), FOV_MIN, FOV_MAX);
		addRenderableWidget(new AbstractSliderButton(x, y, w, 20, Component.empty(), (fov - FOV_MIN) / (double) (FOV_MAX - FOV_MIN)) {
			{
				updateMessage();
			}

			private int fov() {
				return (int) Math.round(FOV_MIN + value * (FOV_MAX - FOV_MIN));
			}

			@Override
			protected void updateMessage() {
				setMessage(Component.literal("FOV: " + fov()));
			}

			@Override
			protected void applyValue() {
				set("fov_scale", String.format(Locale.ROOT, "%.5f", 1f + (fov() - FOV_MIN) * SCALE_PER_DEGREE));
			}
		});
		double sens = Mth.clamp(parse(value("sens", "5"), 5f), SENS_MIN, SENS_MAX);
		addRenderableWidget(new AbstractSliderButton(x + w + 10, y, w, 20, Component.empty(), (sens - SENS_MIN) / (SENS_MAX - SENS_MIN)) {
			{
				updateMessage();
			}

			private double sens() {
				return Math.round((SENS_MIN + value * (SENS_MAX - SENS_MIN)) * 10) / 10.0;
			}

			@Override
			protected void updateMessage() {
				setMessage(Component.literal("Sensitivity: " + sens()));
			}

			@Override
			protected void applyValue() {
				set("sens", Double.toString(sens()));
			}
		});
		y += row;
		addRenderableWidget(cycle(x, y, w, "Autosprint", "autosprint", List.of("0", "1"), "0"));
		addRenderableWidget(Button.builder(Component.literal("HUD layout..."), b -> minecraft.gui.setScreen(new HudEditorScreen(this))).bounds(x + w + 10, y, w, 20).build());
		y += row;
		addRenderableWidget(cycle(x, y, w, "Legend", "legend", Lurch.choices("legend"), "pathfinder"));
		addRenderableWidget(cycle(x + w + 10, y, w, "Heirloom", "heirloom", Lurch.choices("heirloom"), "none"));
		y += row * 2;
		addRenderableWidget(Button.builder(Component.literal("Done"), b -> onClose()).bounds(width / 2 - 100, y, 200, 20).build());
	}

	/** A button that steps the option through `values`. */
	private static Button cycle(int x, int y, int w, String label, String name, List<String> values, String fallback) {
		String[] now = {value(name, fallback)};
		Button b = Button.builder(Component.literal(text(label, name, now[0])), btn -> {
			int i = values.indexOf(now[0]);
			now[0] = values.get((i + 1) % values.size());
			set(name, now[0]);
			btn.setMessage(Component.literal(text(label, name, now[0])));
		}).bounds(x, y, w, 20).build();
		return b;
	}

	private static String text(String label, String name, String v) {
		if (name.equals("autosprint")) {
			v = v.equals("1") ? "On" : "Off";
		} else if (!v.isEmpty()) {
			v = Character.toUpperCase(v.charAt(0)) + v.substring(1).replace('_', ' ');
		}
		return label + ": " + v;
	}

	@Override
	public void onClose() {
		minecraft.gui.setScreen(parent);
	}
}
