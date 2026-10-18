package lurch.mc;

import lurch.mc.LurchConfig.Place;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * Moves and resizes the HUD's pieces over the game: drag a box to move it, drag its corner or
 * turn the wheel over it to resize it. The game keeps running, so lurch-mc's panels follow.
 */
public final class HudEditorScreen extends Screen {
	private static final int CORNER = 6, BOX = 0xFF55FFFF, FILL = 0x3355FFFF, HELD = 0xFFFFFF55;
	private static final float MIN_SCALE = 0.25f, MAX_SCALE = 4f;

	private final Screen parent;
	private String held;
	private boolean resizing;
	private double fromX, fromY;
	private Place start;
	private float[] startRect;

	public HudEditorScreen(Screen parent) {
		super(Component.literal("lurch HUD layout"));
		this.parent = parent;
	}

	@Override
	protected void init() {
		int y = height / 2 - 10;
		addRenderableWidget(Button.builder(Component.literal("Reset"), b -> LurchConfig.resetLayout()).bounds(width / 2 - 102, y, 100, 20).build());
		addRenderableWidget(Button.builder(Component.literal("Done"), b -> onClose()).bounds(width / 2 + 2, y, 100, 20).build());
	}

	@Override
	public boolean isPauseScreen() {
		return false;
	}

	@Override
	public void onClose() {
		LurchConfig.save();
		minecraft.gui.setScreen(parent);
	}

	@Override
	public void extractBackground(GuiGraphicsExtractor g, int mouseX, int mouseY, float partial) {
	}

	private float[] box(String element) {
		float[] r = HudLayout.rect(element);
		return r == null ? null : HudLayout.placed(r, LurchConfig.place(element));
	}

	private String at(double x, double y) {
		for (int i = HudLayout.ALL.length - 1; i >= 0; i--) {
			float[] b = box(HudLayout.ALL[i]);
			if (b != null && x >= b[0] - CORNER && x <= b[2] + CORNER && y >= b[1] - CORNER && y <= b[3] + CORNER) {
				return HudLayout.ALL[i];
			}
		}
		return null;
	}

	@Override
	public void extractRenderState(GuiGraphicsExtractor g, int mouseX, int mouseY, float partial) {
		for (String e : HudLayout.ALL) {
			float[] b = box(e);
			if (b == null) {
				continue;
			}
			int x0 = Math.round(b[0]), y0 = Math.round(b[1]), x1 = Math.max(x0 + 1, Math.round(b[2])), y1 = Math.max(y0 + 1, Math.round(b[3]));
			int line = e.equals(held) ? HELD : BOX;
			g.fill(x0, y0, x1, y1, FILL);
			g.outline(x0, y0, x1 - x0, y1 - y0, line);
			g.fill(x1 - CORNER, y1 - CORNER, x1, y1, line);
			g.text(font, HudLayout.label(e), x0 + 2, y0 + 2, 0xFFFFFFFF, true);
		}
		g.centeredText(font, "Drag to move, drag a corner or scroll to resize", width / 2, height / 2 - 24, 0xFFFFFFFF);
		super.extractRenderState(g, mouseX, mouseY, partial);
	}

	@Override
	public boolean mouseClicked(MouseButtonEvent e, boolean doubleClick) {
		if (super.mouseClicked(e, doubleClick)) {
			return true;
		}
		held = at(e.x(), e.y());
		if (held == null) {
			return false;
		}
		float[] b = box(held);
		resizing = e.x() >= b[2] - CORNER * 2 && e.y() >= b[3] - CORNER * 2;
		fromX = e.x();
		fromY = e.y();
		start = LurchConfig.place(held);
		startRect = b;
		return true;
	}

	@Override
	public boolean mouseDragged(MouseButtonEvent e, double dx, double dy) {
		if (held == null) {
			return super.mouseDragged(e, dx, dy);
		}
		if (resizing) {
			float w = startRect[2] - startRect[0];
			float k = w <= 0 ? 1 : (float) ((w + (e.x() - fromX)) / w);
			LurchConfig.setPlace(held, new Place(start.dx(), start.dy(), clampScale(start.scale() * k)));
		} else {
			LurchConfig.setPlace(held, new Place(start.dx() + (float) ((e.x() - fromX) / width), start.dy() + (float) ((e.y() - fromY) / height), start.scale()));
		}
		return true;
	}

	@Override
	public boolean mouseReleased(MouseButtonEvent e) {
		held = null;
		return super.mouseReleased(e);
	}

	@Override
	public boolean mouseScrolled(double x, double y, double scrollX, double scrollY) {
		String e = at(x, y);
		if (e == null) {
			return super.mouseScrolled(x, y, scrollX, scrollY);
		}
		Place p = LurchConfig.place(e);
		LurchConfig.setPlace(e, new Place(p.dx(), p.dy(), clampScale(p.scale() * (float) Math.pow(1.1, scrollY))));
		return true;
	}

	private static float clampScale(float s) {
		return Math.max(MIN_SCALE, Math.min(MAX_SCALE, s));
	}
}
