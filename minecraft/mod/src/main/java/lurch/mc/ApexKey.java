package lurch.mc;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.KeyMapping;
import net.minecraft.resources.Identifier;

/**
 * One of lurch's actions as a key in Minecraft's Controls screen, under "Apex (lurch)". Minecraft
 * keeps and saves the key; lurch-mc binds the action to it (`bind` in lurch-mc's protocol).
 */
public final class ApexKey extends KeyMapping {
	private static final Category CATEGORY = Category.register(Identifier.fromNamespaceAndPath("lurch", "apex"));

	public static final ApexKey[] ALL = {
		new ApexKey("tactical", InputConstants.KEY_Q),
		new ApexKey("ultimate", InputConstants.KEY_Z),
		new ApexKey("jump", InputConstants.KEY_SPACE),
		new ApexKey("crouch", InputConstants.KEY_LCONTROL),
	};

	/** Opens lurch's options (`LurchOptionsScreen`); Minecraft's key, not one lurch-mc binds. */
	public static final KeyMapping OPTIONS = new KeyMapping("key.lurch.options", InputConstants.Type.KEYBOARD, InputConstants.KEY_O, CATEGORY);

	/** lurch-mc's name for the action. */
	public final String action;

	private ApexKey(String action, int key) {
		super("key.lurch." + action, InputConstants.Type.KEYBOARD, key, CATEGORY);
		this.action = action;
	}

	/** lurch-mc's `bind` line for this key. */
	public String bindLine() {
		String to = key.getType() == InputConstants.Type.MOUSE ? "m " + key.getValue() : isUnbound() ? "-" : "k " + key.getValue();
		return "bind " + action + " " + to + "\n";
	}

	/** Whether the key or mouse button is one of lurch's keys. */
	public static boolean any(InputConstants.Key key) {
		for (ApexKey k : ALL) {
			if (k.matches(key)) {
				return true;
			}
		}
		return false;
	}

	/** Whether the key event is one of lurch's keys. */
	public static boolean any(net.minecraft.client.input.KeyEvent e) {
		for (ApexKey k : ALL) {
			if (k.matches(e)) {
				return true;
			}
		}
		return false;
	}
}
