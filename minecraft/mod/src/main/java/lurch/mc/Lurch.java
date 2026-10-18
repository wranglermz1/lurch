package lurch.mc;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.ByteOrder;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

import com.mojang.blaze3d.platform.InputConstants;
import lurch.mc.mixin.NativeImageAccess;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.LightmapRenderState;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.level.CardinalLighting;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3fc;
import org.lwjgl.system.MemoryUtil;

/**
 * The Minecraft half of the passthrough: while F7 has it on, lurch-mc (the game shell) gets the
 * keyboard and mouse, the blocks around the player and one frame per rendered frame, and places
 * the player and the camera; its viewmodel and HUD are drawn over the world from shared memory.
 */
public final class Lurch {
	private static final int PORT = 25600;
	private static final Path SHM = System.getProperty("os.name").startsWith("Windows") ? Path.of(System.getProperty("java.io.tmpdir"), "lurch-mc") : Path.of("/dev/shm/lurch-mc");
	private static final Identifier OVERLAY = Identifier.fromNamespaceAndPath("lurch", "overlay");
	private static final int HORIZONTAL = 24, BELOW = 8, ABOVE = 24;
	private static final long SCAN_EVERY_NS = 500_000_000L;
	private static final int RESCAN_DISTANCE = 4;
	/** How far from the player mobs are sent: past the grapple's 850 units, 21¼ blocks. */
	private static final double BODY_RANGE = 24;
	/** A punch hits three times as hard as Minecraft's empty hand: its attack damage, tripled while it lands. */
	private static final Identifier PUNCH = Identifier.fromNamespaceAndPath("lurch", "punch");
	private static final double PUNCH_EXTRA = 2.0;

	private static Socket socket;
	private static BufferedReader in;
	private static Writer out;
	private static final StringBuilder events = new StringBuilder();
	private static long lastFrame;
	private static boolean toggleHeld, capturing;
	/** The mouse buttons down that went to lurch. */
	private static final Set<Integer> lurchButtons = new HashSet<>();
	/** Where lurch last put the player; anything else moving it (a teleport, a respawn) restarts lurch there. */
	private static Vec3 placed;
	private static String lastBlocks = "";
	private static BlockPos scannedAt;
	private static long scannedTime;
	/** A block that blocks movement changed since the last scan. */
	private static boolean blocksChanged;

	/** The camera lurch asks for; null while lurch is off. */
	public static Vec3 eye;
	public static float pitch, yaw, roll, fov;

	/** The lightmap's inputs as Minecraft last rendered it, and the last light sent. */
	private static float skyFactor, blockFactor, nightVision, darkness, bossDarkening, brightness;
	private static final float[] blockTint = {1, 1, 1}, skyColor = {1, 1, 1}, ambient = new float[3], nightVisionColor = new float[3];
	private static String lastLight = "", lastHotbar = "", lastBinds = "", lastHands = "", lastLayout = "";
	private static boolean sentOptions;
	/**
	 * The player is waist-deep in water: Minecraft moves them, swimming, until they are out, and
	 * lurch starts again where they are. Apex has no swimming.
	 */
	private static boolean swimming;
	/** How far above the feet water must reach for Minecraft to take over: half Steve's height, so lurch still wades through one block of water. */
	private static final double WAIST = 0.9;
	/** The player's speed when lurch last placed them, in blocks per tick. */
	private static Vec3 lastVelocity = Vec3.ZERO;
	/** lurch-mc's options as it last said (`config`), and what `set` may choose from (`choices`). */
	private static final Map<String, String> hostOptions = new HashMap<>();
	private static final Map<String, List<String>> choices = new HashMap<>();
	/** Where lurch-mc draws its panels before the player's layout, in window pixels (`rect`). */
	private static final Map<String, float[]> rects = new HashMap<>();
	/** Whether lurch-mc drew arms in its last frame; Minecraft draws its own hand only when it did not. */
	public static boolean arms = true;
	/**
	 * The box in the Apex player panel that the armour, hearts and hunger go in, as left, top, right,
	 * bottom, and how far left the hotbar may reach, in window pixels, as lurch-mc last said; null
	 * and NaN until it has.
	 */
	private static float[] panel;
	private static float room = Float.NaN;
	/** The hotbar's half width with the offhand slot beside it (`Hud.extractItemHotbar`), in GUI pixels. */
	private static final int HOTBAR_HALF = 91 + 29;
	/** The rows of icons in the player panel, ten sprite pixels apart, and the air row above it. */
	public static final int AIR = -1, ARMOR = 0, HEARTS = 1, FOOD = 2;
	/** Three rows of 9-pixel icons ten apart are 29 sprite pixels; the panel takes as many window pixels per sprite pixel as leave a margin. */
	private static final int ROWS_HEIGHT = 29, PANEL_PER_PIXEL = 32;

	private static MappedByteBuffer shm;
	private static DynamicTexture overlay;
	private static int overlaySeq;

	/** Where lurch last put the player, read by the integrated server's thread; null while lurch is off or the player swims. */
	public static volatile Vec3 placedAt;

	/** The player lurch drives, read by the integrated server's thread; null while lurch is off. */
	private static volatile UUID driven;

	/** Whether lurch drives the player: it is on and the player is not swimming. */
	public static boolean on() {
		return socket != null && !swimming;
	}

	public static boolean drives(UUID player) {
		return player.equals(driven);
	}

	public static boolean moves(Object entity) {
		return on() && entity instanceof LocalPlayer;
	}

	/** Whether the entity aims from lurch's camera: the player lurch moves, once lurch has placed the camera. */
	public static boolean aims(Object entity) {
		return eye != null && moves(entity);
	}

	/** Whether input goes to lurch instead of Minecraft: lurch is on and no screen is open. */
	private static boolean ours() {
		Minecraft mc = Minecraft.getInstance();
		return on() && mc.gui.screen() == null && mc.player != null;
	}

	/** Minecraft's keys that stay Minecraft's while lurch is on: everything but moving, which lurch's keys do. */
	private static List<KeyMapping> minecraftKeys(Minecraft mc) {
		var o = mc.options;
		List<KeyMapping> keys = new java.util.ArrayList<>(List.of(o.keyChat, o.keyCommand, o.keyInventory, o.keySwapOffhand, o.keyDrop, o.keyUse, o.keyAttack, o.keyPickItem, o.keyPlayerList, o.keyAdvancements, o.keySocialInteractions, o.keyFriends, o.keyQuickActions));
		keys.addAll(List.of(o.keyHotbarSlots));
		keys.add(ApexKey.OPTIONS);
		return keys;
	}

	/**
	 * True when Minecraft should not see this key. Escape, F1 to F12 and Minecraft's own keys but
	 * its movement (`minecraftKeys`: chat, inventory, drop, the hotbar's slots and so on) stay
	 * Minecraft's, unless one of lurch's keys (`ApexKey`) is bound there.
	 */
	public static boolean key(int action, KeyEvent e) {
		Minecraft mc = Minecraft.getInstance();
		int k = e.key();
		if (!ours() || k == InputConstants.KEY_ESCAPE || (k >= InputConstants.KEY_F1 && k <= InputConstants.KEY_F12)) {
			return false;
		}
		if (!ApexKey.any(e)) {
			for (KeyMapping m : minecraftKeys(mc)) {
				if (m.matches(e)) {
					return false;
				}
			}
		}
		if (action != InputConstants.REPEAT) {
			events.append("k ").append(k).append(action == InputConstants.PRESS ? " 1\n" : " 0\n");
		}
		return true;
	}

	/**
	 * True when Minecraft should not see this mouse button. lurch takes the left button while the
	 * fists are out (a punch) and any button one of lurch's keys is bound to; Minecraft keeps the
	 * rest, so the player mines, attacks, places and uses items as usual. A button goes up to
	 * whichever side it went down to.
	 */
	public static boolean button(int button, int action) {
		Minecraft mc = Minecraft.getInstance();
		if (action == InputConstants.RELEASE && (!lurchButtons.remove(button) || !ours())) {
			return false;
		}
		if (action == InputConstants.PRESS) {
			if (!ours()) {
				return false;
			}
			boolean apex = ApexKey.any(InputConstants.Type.MOUSE.getOrCreate(button));
			boolean punch = button == InputConstants.MOUSE_BUTTON_LEFT && mc.player.getMainHandItem().isEmpty();
			if (!apex && !punch) {
				return false;
			}
			lurchButtons.add(button);
		}
		events.append("m ").append(button).append(action == InputConstants.PRESS ? " 1\n" : " 0\n");
		return true;
	}

	/**
	 * A punch's blow landed on what lurch-mc's trace hit (`punch` in its protocol). A mob takes
	 * three times the damage Minecraft's empty hand deals it: on the integrated server the
	 * player's attack damage is tripled while the attack lands (singleplayer only; elsewhere the
	 * hand's own attack). A block breaks, with its drops, when an empty hand mines it at least as
	 * fast as it mines a log, and in Creative always; anything slower is left as it is. Breaking
	 * goes through the integrated server, so in singleplayer only.
	 */
	private static void punch(Minecraft mc, String[] r) {
		LocalPlayer p = mc.player;
		IntegratedServer server = mc.getSingleplayerServer();
		UUID id = p.getUUID();
		if (r[1].equals("e")) {
			int target = Integer.parseInt(r[2]);
			Entity e = mc.level.getEntity(target);
			if (e == null) {
				return;
			}
			if (server == null) {
				mc.gameMode.attack(p, e);
				return;
			}
			server.execute(() -> {
				ServerPlayer sp = server.getPlayerList().getPlayer(id);
				Entity t = sp == null ? null : sp.level().getEntity(target);
				if (t == null) {
					return;
				}
				AttributeInstance damage = sp.getAttribute(Attributes.ATTACK_DAMAGE);
				damage.addOrUpdateTransientModifier(new AttributeModifier(PUNCH, PUNCH_EXTRA, AttributeModifier.Operation.ADD_MULTIPLIED_TOTAL));
				try {
					sp.attack(t);
				} finally {
					damage.removeModifier(PUNCH);
				}
			});
			return;
		}
		if (!r[1].equals("b") || server == null) {
			return;
		}
		BlockPos pos = new BlockPos(Integer.parseInt(r[2]), Integer.parseInt(r[3]), Integer.parseInt(r[4]));
		BlockState seen = mc.level.getBlockState(pos);
		server.execute(() -> {
			ServerPlayer sp = server.getPlayerList().getPlayer(id);
			if (sp == null) {
				return;
			}
			ServerLevel level = sp.level();
			BlockState s = level.getBlockState(pos);
			if (s.isAir() || !(sp.isCreative() || s.getDestroyProgress(sp, level, pos) >= Blocks.OAK_LOG.defaultBlockState().getDestroyProgress(sp, level, pos))) {
				return;
			}
			if (sp.gameMode.destroyBlock(pos)) {
				mc.execute(() -> mc.level.levelEvent(null, 2001, pos, Block.getId(seen)));
			}
		});
	}

	/**
	 * A block changed in the client's world (`ClientLevelMixin`): one that blocks or blocked
	 * movement near the player rescans the blocks the next frame, so a block just placed is
	 * floor before the pilot falls through it.
	 */
	public static void blockChanged(ClientLevel level, BlockPos pos, BlockState old, BlockState now) {
		if (socket == null || scannedAt == null || Math.abs(pos.getX() - scannedAt.getX()) > HORIZONTAL || Math.abs(pos.getZ() - scannedAt.getZ()) > HORIZONTAL) {
			return;
		}
		if (!old.getCollisionShape(level, pos).isEmpty() || !now.getCollisionShape(level, pos).isEmpty()) {
			blocksChanged = true;
		}
	}

	/** The mobs near the player, as lurch-mc's `bodies` message. */
	private static String bodies(ClientLevel level, LocalPlayer p) {
		StringBuilder sb = new StringBuilder("bodies\n");
		for (Entity e : level.getEntities(p, p.getBoundingBox().inflate(BODY_RANGE), e -> e instanceof LivingEntity && e.isAlive() && !e.isSpectator())) {
			AABB b = e.getBoundingBox();
			sb.append("b ").append(e.getId()).append(' ').append(b.minX).append(' ').append(b.minY).append(' ').append(b.minZ).append(' ').append(b.maxX).append(' ').append(b.maxY).append(' ').append(b.maxZ).append('\n');
		}
		return sb.append("end\n").toString();
	}

	/** The player's place for each of lurch-mc's panels, as `layout` lines in window pixels. */
	private static String layoutLines(Minecraft mc) {
		var win = mc.getWindow();
		StringBuilder sb = new StringBuilder();
		for (String e : HudLayout.APEX) {
			LurchConfig.Place p = LurchConfig.place(e);
			sb.append("layout ").append(e).append(' ').append(p.dx() * win.getWidth()).append(' ').append(p.dy() * win.getHeight()).append(' ').append(p.scale()).append('\n');
		}
		return sb.toString();
	}

	/** lurch-mc's value for an option, or null before it said. */
	public static String hostOption(String name) {
		return hostOptions.get(name);
	}

	/** What an option may be, as lurch-mc last said, or the install's usual choices before it has. */
	public static List<String> choices(String name) {
		List<String> c = choices.get(name);
		if (c != null) {
			return c;
		}
		return name.equals("legend") ? List.of("bangalore", "bloodhound", "caustic", "crypto", "gibraltar", "lifeline", "mirage", "octane", "pathfinder", "revenant", "wattson", "wraith") : List.of("none", "wraith_kunai");
	}

	/** Whether water reaches the player's waist. */
	private static boolean waistDeep(ClientLevel level, LocalPlayer p) {
		double y = p.getY() + WAIST;
		BlockPos at = BlockPos.containing(p.getX(), y, p.getZ());
		FluidState f = level.getFluidState(at);
		return f.is(FluidTags.WATER) && y < at.getY() + f.getHeight(level, at);
	}

	/**
	 * Hands the player to Minecraft in water, at the speed lurch moved them, with the keys held
	 * now; and back to lurch out of it, which starts where they are (a `spawn`) with the keys
	 * held then. lurch-mc lets go of every key first (`r`), since it saw none of the swim's.
	 */
	private static void swim(Minecraft mc, LocalPlayer p, boolean in) {
		swimming = in;
		placed = null;
		placedAt = null;
		if (in) {
			eye = null;
			p.setDeltaMovement(lastVelocity);
			KeyMapping.setAll();
			events.append("r\n");
			return;
		}
		KeyMapping.releaseAll();
		for (int k = 0; k < 512; k++) {
			if (InputConstants.isKeyDown(k)) {
				key(InputConstants.PRESS, new KeyEvent(k, 0, 0));
			}
		}
	}

	/** Sends an option the player changed to lurch-mc, while it is connected. */
	public static void sendOption(String name, String value) {
		if (socket != null) {
			events.append("set ").append(name).append(' ').append(value).append('\n');
		}
	}

	/** Where lurch-mc draws one of its panels before the player's layout, in GUI pixels, or null. */
	public static float[] apexRect(String element) {
		float[] r = rects.get(element);
		if (!on() || r == null) {
			return null;
		}
		float k = Minecraft.getInstance().getWindow().getGuiScale();
		return new float[] {r[0] / k, r[1] / k, r[2] / k, r[3] / k};
	}

	public static boolean scroll(double y) {
		if (!ours()) {
			return false;
		}
		events.append("w ").append(y).append('\n');
		return true;
	}

	public static boolean move(double dx, double dy) {
		if (!ours() || !Minecraft.getInstance().mouseHandler.isMouseGrabbed()) {
			return false;
		}
		events.append("l ").append(dx).append(' ').append(dy).append('\n');
		return true;
	}

	public static void frame(Minecraft mc) {
		boolean toggle = mc.gui.screen() == null && InputConstants.isKeyDown(InputConstants.KEY_F7);
		if (toggle && !toggleHeld) {
			if (socket == null) {
				start(mc);
			} else {
				stop(mc, "off");
			}
		}
		toggleHeld = toggle;
		while (ApexKey.OPTIONS.consumeClick()) {
			if (mc.gui.screen() == null) {
				mc.gui.setScreen(new LurchOptionsScreen(null));
			}
		}
		LocalPlayer p = mc.player;
		if (socket == null) {
			return;
		}
		if (p == null || mc.level == null) {
			stop(mc, null);
			return;
		}
		long now = System.nanoTime();
		double frametime = (now - lastFrame) / 1e9;
		lastFrame = now;
		if (mc.isPaused()) {
			return;
		}
		boolean wet = waistDeep(mc.level, p);
		if (wet != swimming) {
			swim(mc, p, wet);
		}
		if (swimming) {
			return;
		}
		boolean nowOurs = ours();
		if (capturing && !nowOurs) {
			events.append("r\n");
		}
		capturing = nowOurs;
		try {
			StringBuilder msg = new StringBuilder();
			if (placed == null || p.position().distanceToSqr(placed) > 1e-4) {
				msg.append(blocks(mc.level, p.blockPosition(), true));
				msg.append("spawn ").append(p.getX()).append(' ').append(p.getY()).append(' ').append(p.getZ()).append(' ').append(Mth.wrapDegrees(p.getYRot())).append(' ').append(mc.level.getMinY()).append('\n');
			} else {
				msg.append(blocks(mc.level, p.blockPosition(), false));
			}
			StringBuilder binds = new StringBuilder();
			for (ApexKey key : ApexKey.ALL) {
				binds.append(key.bindLine());
			}
			if (!binds.toString().equals(lastBinds)) {
				msg.append(binds);
				lastBinds = binds.toString();
			}
			String hands = p.getMainHandItem().isEmpty() ? "hands 1\n" : "hands 0\n";
			if (!hands.equals(lastHands)) {
				msg.append(hands);
				lastHands = hands;
			}
			String hotbar = hotbarLine(mc);
			if (!hotbar.equals(lastHotbar)) {
				msg.append(hotbar);
				lastHotbar = hotbar;
			}
			String light = light(mc.level, p);
			if (!light.equals(lastLight)) {
				msg.append(light);
				lastLight = light;
			}
			if (!sentOptions) {
				msg.append(LurchConfig.setLines());
				sentOptions = true;
			}
			String layout = layoutLines(mc);
			if (!layout.equals(lastLayout)) {
				msg.append(layout);
				lastLayout = layout;
			}
			msg.append(bodies(mc.level, p));
			msg.append(events);
			events.setLength(0);
			msg.append("frame ").append(frametime).append(' ').append(mc.getWindow().getWidth()).append(' ').append(mc.getWindow().getHeight()).append('\n');
			out.write(msg.toString());
			out.flush();
			String line = in.readLine();
			List<String[]> punches = new ArrayList<>();
			while (line != null && !line.startsWith("pose ")) {
				String[] r = line.split(" ");
				switch (r[0]) {
					case "hud" -> layout(r);
					case "rect" -> rects.put(r[1], r[2].equals("-") ? null : new float[] {Float.parseFloat(r[2]), Float.parseFloat(r[3]), Float.parseFloat(r[4]), Float.parseFloat(r[5])});
					case "punch" -> punches.add(r);
					case "choices" -> choices.put(r[1], List.of(Arrays.copyOfRange(r, 2, r.length)));
					case "config" -> hostOptions.put(r[1], r[2]);
					default -> {
					}
				}
				line = in.readLine();
			}
			if (line == null) {
				throw new IOException("lurch-mc closed the connection");
			}
			String[] r = line.split(" ");
			double x = Double.parseDouble(r[1]), y = Double.parseDouble(r[2]), z = Double.parseDouble(r[3]);
			p.setPos(x, y, z);
			p.xo = x;
			p.yo = y;
			p.zo = z;
			p.setDeltaMovement(Vec3.ZERO);
			p.setOnGround(r[13].equals("1"));
			arms = r[14].equals("1");
			float viewPitch = Float.parseFloat(r[11]), viewYaw = Float.parseFloat(r[12]);
			p.setXRot(viewPitch);
			p.setYRot(viewYaw);
			p.xRotO = viewPitch;
			p.yRotO = viewYaw;
			p.setYHeadRot(viewYaw);
			if (placed != null && frametime > 0) {
				lastVelocity = p.position().subtract(placed).scale(1 / (frametime * 20));
			}
			placed = p.position();
			placedAt = placed;
			eye = new Vec3(Double.parseDouble(r[4]), Double.parseDouble(r[5]), Double.parseDouble(r[6]));
			pitch = Float.parseFloat(r[7]);
			yaw = Float.parseFloat(r[8]);
			roll = Float.parseFloat(r[9]);
			fov = Float.parseFloat(r[10]);
			for (String[] blow : punches) {
				punch(mc, blow);
			}
			readOverlay(mc);
		} catch (IOException | RuntimeException e) {
			stop(mc, e.toString());
		}
	}

	private static void layout(String[] r) {
		panel = r[1].equals("-") ? null : new float[] {Float.parseFloat(r[1]), Float.parseFloat(r[2]), Float.parseFloat(r[3]), Float.parseFloat(r[4])};
		room = r[5].equals("-") ? Float.NaN : Float.parseFloat(r[5]);
	}

	/**
	 * Where the hotbar goes while lurch has placed the player panel: centred, standing on the panel's
	 * bottom, at the GUI scale or less, as many window pixels per GUI pixel as leave lurch-mc its
	 * room. As {window pixels per GUI pixel, centre x, bottom y} in window pixels, or null.
	 */
	private static int[] hotbar() {
		float[] p = panel;
		if (!on() || p == null) {
			return null;
		}
		var w = Minecraft.getInstance().getWindow();
		int scale = w.getGuiScale(), centre = w.getWidth() / 2;
		int k = Float.isNaN(room) ? scale : Mth.clamp((int) ((centre - room) / HOTBAR_HALF), 1, scale);
		return new int[] {k, centre, Math.round(p[3])};
	}

	private static String hotbarLine(Minecraft mc) {
		int[] h = hotbar();
		return h == null ? "" : "hotbar " + (h[1] - HOTBAR_HALF * h[0]) + "\n";
	}

	/**
	 * Pushes a pose that draws `element` (the hotbar, or the experience bar, which moves with it)
	 * where the player's layout puts it, from where `hotbar` puts the hotbar; false, with nothing
	 * pushed, when neither moves it.
	 */
	public static boolean pushHotbar(GuiGraphicsExtractor g, String element) {
		int[] h = hotbar();
		boolean placed = HudLayout.push(g, element);
		if (h == null) {
			return placed;
		}
		if (!placed) {
			g.pose().pushMatrix();
		}
		float scale = Minecraft.getInstance().getWindow().getGuiScale();
		g.pose().translate((h[1] - h[0] * (g.guiWidth() / 2)) / scale, (h[2] - h[0] * g.guiHeight()) / scale);
		g.pose().scale(h[0] / scale);
		return true;
	}

	/** Where the hotbar's pose (`pushHotbar` without the layout) puts a rectangle in GUI pixels. */
	public static float[] throughHotbar(float[] r) {
		int[] h = hotbar();
		if (h == null) {
			return r;
		}
		var w = Minecraft.getInstance().getWindow();
		float scale = w.getGuiScale(), k = h[0] / scale;
		float tx = (h[1] - h[0] * (w.getGuiScaledWidth() / 2)) / scale, ty = (h[2] - h[0] * w.getGuiScaledHeight()) / scale;
		return new float[] {r[0] * k + tx, r[1] * k + ty, r[2] * k + tx, r[3] * k + ty};
	}

	/**
	 * Pushes a pose that draws Minecraft's row of icons whose first icon is at (x, y), in GUI
	 * pixels, as row `row` of the player panel, at a whole number of window pixels per sprite
	 * pixel. False, with nothing pushed, while lurch is off or has not placed the panel.
	 */
	public static boolean pushPanelRow(GuiGraphicsExtractor g, int row, int x, int y) {
		float[] p = panel;
		if (!on() || p == null) {
			return false;
		}
		int scale = Minecraft.getInstance().getWindow().getGuiScale();
		int px = Math.max(1, (int) ((p[3] - p[1]) / PANEL_PER_PIXEL));
		int top = Math.round((p[1] + p[3] - ROWS_HEIGHT * px) / 2f);
		g.pose().pushMatrix();
		g.pose().translate(Math.round(p[0]) / (float) scale, (top + row * 10 * px) / (float) scale);
		g.pose().scale(px / (float) scale);
		g.pose().translate(-x, -y);
		return true;
	}

	public static void lightmap(LightmapRenderState s) {
		skyFactor = s.skyFactor;
		blockFactor = s.blockFactor;
		nightVision = s.nightVisionEffectIntensity;
		darkness = s.darknessEffectScale;
		bossDarkening = s.bossOverlayWorldDarkening;
		brightness = s.brightness;
		copy(s.blockLightTint, blockTint);
		copy(s.skyLightColor, skyColor);
		copy(s.ambientColor, ambient);
		copy(s.nightVisionColor, nightVisionColor);
	}

	private static void copy(Vector3fc v, float[] to) {
		if (v != null) {
			to[0] = v.x();
			to[1] = v.y();
			to[2] = v.z();
		}
	}

	private static float lightBrightness(float level) {
		return level / (4f - 3f * level);
	}

	/**
	 * The lightmap texel Minecraft's hand is multiplied by, for the light at the player's eye:
	 * `lightmap.fsh` on the CPU, and the eye's light as `EntityRenderer.getPackedLightCoords`
	 * reads it.
	 */
	private static String light(ClientLevel level, LocalPlayer p) {
		BlockPos eyeAt = BlockPos.containing(p.getEyePosition());
		float block = (p.isOnFire() ? 15 : level.getBrightness(LightLayer.BLOCK, eyeAt)) / 15f;
		float sky = level.getBrightness(LightLayer.SKY, eyeAt) / 15f;
		float blockBrightness = lightBrightness(block) * blockFactor, skyBrightness = lightBrightness(sky) * skyFactor;
		float mix = 0.9f * (2f * block - 1f) * (2f * block - 1f);
		float[] boss = {0.7f, 0.6f, 0.6f};
		float[] c = new float[3];
		for (int i = 0; i < 3; i++) {
			float v = Math.max(ambient[i], nightVisionColor[i] * nightVision);
			v += skyColor[i] * skyBrightness;
			v += (blockTint[i] + (1f - blockTint[i]) * mix) * blockBrightness;
			v = v + (v * boss[i] - v) * bossDarkening;
			c[i] = Mth.clamp(v - darkness, 0f, 1f);
		}
		float max = Math.max(Math.max(c[0], c[1]), c[2]);
		float scaled = 1f - (1f - max) * (1f - max) * (1f - max) * (1f - max);
		StringBuilder sb = new StringBuilder("light");
		for (int i = 0; i < 3; i++) {
			float notGamma = max > 0f ? c[i] * (scaled / max) : 0f;
			sb.append(' ').append(c[i] + (notGamma - c[i]) * brightness);
		}
		boolean nether = level.dimensionType().cardinalLightType() == CardinalLighting.Type.NETHER;
		return sb.append(nether ? " 1\n" : " 0\n").toString();
	}

	/** The blocks near the player when they changed, or when asked, as lurch-mc's `blocks` message. */
	private static String blocks(ClientLevel level, BlockPos at, boolean force) {
		long now = System.nanoTime();
		if (!force && !blocksChanged && now - scannedTime < SCAN_EVERY_NS && at.distManhattan(scannedAt) < RESCAN_DISTANCE) {
			return "";
		}
		blocksChanged = false;
		scannedTime = now;
		scannedAt = at;
		int x0 = at.getX() - HORIZONTAL - 1, y0 = at.getY() - BELOW - 1, z0 = at.getZ() - HORIZONTAL - 1;
		int nx = 2 * HORIZONTAL + 3, ny = BELOW + ABOVE + 3, nz = nx;
		boolean[] full = new boolean[nx * ny * nz];
		BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
		for (int i = 0; i < nx; i++) {
			for (int j = 0; j < ny; j++) {
				for (int k = 0; k < nz; k++) {
					pos.set(x0 + i, y0 + j, z0 + k);
					BlockState s = level.getBlockState(pos);
					full[(i * ny + j) * nz + k] = !s.isAir() && s.isCollisionShapeFullBlock(level, pos);
				}
			}
		}
		StringBuilder sb = new StringBuilder("blocks\n");
		int[][] sides = {{-1, 0, 0}, {1, 0, 0}, {0, -1, 0}, {0, 1, 0}, {0, 0, -1}, {0, 0, 1}};
		for (int i = 1; i < nx - 1; i++) {
			for (int j = 1; j < ny - 1; j++) {
				for (int k = 1; k < nz - 1; k++) {
					int x = x0 + i, y = y0 + j, z = z0 + k;
					if (full[(i * ny + j) * nz + k]) {
						int mask = 0;
						for (int f = 0; f < 6; f++) {
							int[] d = sides[f];
							if (!full[((i + d[0]) * ny + j + d[1]) * nz + k + d[2]]) {
								mask |= 1 << f;
							}
						}
						if (mask != 0) {
							sb.append("f ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(mask).append('\n');
						}
						continue;
					}
					pos.set(x, y, z);
					BlockState s = level.getBlockState(pos);
					if (s.isAir()) {
						continue;
					}
					for (AABB b : s.getCollisionShape(level, pos).toAabbs()) {
						sb.append("p ").append(x).append(' ').append(y).append(' ').append(z).append(' ')
							.append(b.minX).append(' ').append(b.minY).append(' ').append(b.minZ).append(' ')
							.append(b.maxX).append(' ').append(b.maxY).append(' ').append(b.maxZ).append('\n');
					}
				}
			}
		}
		sb.append("end\n");
		String s = sb.toString();
		if (!force && s.equals(lastBlocks)) {
			return "";
		}
		lastBlocks = s;
		return s;
	}

	/** Copies lurch-mc's newest finished image into the overlay texture. */
	private static void readOverlay(Minecraft mc) throws IOException {
		if (shm == null || shm.capacity() < 20) {
			try (FileChannel ch = FileChannel.open(SHM, StandardOpenOption.READ)) {
				shm = ch.map(FileChannel.MapMode.READ_ONLY, 0, ch.size());
				shm.order(ByteOrder.LITTLE_ENDIAN);
			}
			if (shm.capacity() < 20) {
				return;
			}
		}
		int seq = shm.getInt(4);
		if ((seq & 1) != 0 || seq == overlaySeq) {
			return;
		}
		int w = shm.getInt(8), h = shm.getInt(12), stride = shm.getInt(16);
		if (shm.capacity() < 20L + (long) stride * h) {
			shm = null;
			return;
		}
		if (overlay == null || overlay.getPixels().getWidth() != w || overlay.getPixels().getHeight() != h) {
			overlay = new DynamicTexture("lurch overlay", w, h, false);
			mc.getTextureManager().register(OVERLAY, overlay);
		}
		long src = MemoryUtil.memAddress(shm) + 20, dst = ((NativeImageAccess) (Object) overlay.getPixels()).lurch$pixels();
		for (int row = 0; row < h; row++) {
			MemoryUtil.memCopy(src + (long) row * stride, dst + (long) row * w * 4, (long) w * 4);
		}
		if (shm.getInt(4) == seq) {
			overlay.upload();
			overlaySeq = seq;
		}
	}

	public static void drawOverlay(GuiGraphicsExtractor g) {
		if (!on() || overlay == null) {
			return;
		}
		int w = overlay.getPixels().getWidth(), h = overlay.getPixels().getHeight();
		g.blit(RenderPipelines.GUI_TEXTURED_PREMULTIPLIED_ALPHA, OVERLAY, 0, 0, 0f, 0f, g.guiWidth(), g.guiHeight(), w, h, w, h);
	}

	private static void start(Minecraft mc) {
		if (mc.player == null) {
			return;
		}
		try {
			socket = new Socket();
			socket.setTcpNoDelay(true);
			socket.setSoTimeout(5000);
			socket.connect(new InetSocketAddress("127.0.0.1", PORT), 1000);
			in = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
			out = new OutputStreamWriter(socket.getOutputStream(), StandardCharsets.US_ASCII);
			placed = null;
			lastLight = "";
			panel = null;
			room = Float.NaN;
			lastHotbar = "";
			lastBinds = "";
			lastHands = "";
			lastLayout = "";
			sentOptions = false;
			swimming = false;
			lastVelocity = Vec3.ZERO;
			rects.clear();
			arms = true;
			shm = null;
			capturing = false;
			lurchButtons.clear();
			events.setLength(0);
			lastFrame = System.nanoTime();
			KeyMapping.releaseAll();
			driven = mc.player.getUUID();
			mc.player.sendSystemMessage(Component.literal("lurch: on"));
		} catch (IOException e) {
			stop(mc, "no lurch-mc on 127.0.0.1:" + PORT + " (" + e.getMessage() + ")");
		}
	}

	private static void stop(Minecraft mc, String why) {
		try {
			if (socket != null) {
				socket.close();
			}
		} catch (IOException ignored) {
		}
		socket = null;
		driven = null;
		placedAt = null;
		in = null;
		out = null;
		eye = null;
		if (why != null && mc.player != null) {
			mc.player.sendSystemMessage(Component.literal("lurch: " + why));
		}
	}
}
