package lurch.mc;

import lurch.mc.LurchConfig.Place;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.ChatComponent;

/**
 * The HUD's pieces the player can move and resize (`HudEditorScreen`): lurch-mc's three Apex panels,
 * which lurch-mc draws where `layout` says, and Minecraft's own, which the HUD's mixin draws at
 * their place. Rectangles are {left, top, right, bottom} in GUI pixels.
 */
public final class HudLayout {
	public static final String[] ALL = {"player", "tactical", "ultimate", "hotbar", "xp", "health", "effects", "bosses", "chat"};
	/** The pieces lurch-mc draws. */
	public static final String[] APEX = {"player", "tactical", "ultimate"};

	public static String label(String element) {
		return switch (element) {
			case "player" -> "Apex player panel";
			case "tactical" -> "Tactical";
			case "ultimate" -> "Ultimate";
			case "hotbar" -> "Hotbar";
			case "xp" -> "Experience";
			case "health" -> "Health and hunger";
			case "effects" -> "Effects";
			case "bosses" -> "Boss bars";
			default -> "Chat";
		};
	}

	/** Where the piece is drawn before the player's place, or null while it is not drawn. */
	public static float[] rect(String element) {
		Minecraft mc = Minecraft.getInstance();
		var win = mc.getWindow();
		float w = win.getGuiScaledWidth(), h = win.getGuiScaledHeight();
		return switch (element) {
			case "player", "tactical", "ultimate" -> Lurch.apexRect(element);
			case "hotbar" -> Lurch.throughHotbar(new float[] {w / 2 - 91 - 29, h - 23, w / 2 + 91, h});
			case "xp" -> Lurch.throughHotbar(new float[] {w / 2 - 91, h - 36, w / 2 + 91, h - 24});
			case "health" -> Lurch.on() ? null : new float[] {w / 2 - 91, h - 50, w / 2 + 91, h - 30};
			case "effects" -> new float[] {w - 155, 0, w, 52};
			case "bosses" -> new float[] {w / 2 - 91, 0, w / 2 + 91, 31};
			default -> {
				float cw = ChatComponent.getWidth(mc.options.chatWidth().get()) + 8, ch = ChatComponent.getHeight(mc.options.chatHeightUnfocused().get());
				yield new float[] {0, h - 40 - ch, cw, h - 40};
			}
		};
	}

	/** Where the place puts a piece drawn at `r`. */
	public static float[] placed(float[] r, Place p) {
		var win = Minecraft.getInstance().getWindow();
		float x = r[0] + p.dx() * win.getGuiScaledWidth(), y = r[1] + p.dy() * win.getGuiScaledHeight();
		return new float[] {x, y, x + (r[2] - r[0]) * p.scale(), y + (r[3] - r[1]) * p.scale()};
	}

	/**
	 * Pushes the pose that draws the piece at its place, drawn at `r` before it; false, with
	 * nothing pushed, when the player has not moved it or it is not drawn.
	 */
	public static boolean push(GuiGraphicsExtractor g, String element, float[] r) {
		Place p = LurchConfig.place(element);
		if (!p.moved() || r == null) {
			return false;
		}
		float[] to = placed(r, p);
		g.pose().pushMatrix();
		g.pose().translate(to[0], to[1]);
		g.pose().scale(p.scale());
		g.pose().translate(-r[0], -r[1]);
		return true;
	}

	public static boolean push(GuiGraphicsExtractor g, String element) {
		return push(g, element, rect(element));
	}
}
