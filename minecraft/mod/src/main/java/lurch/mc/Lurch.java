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
import java.util.UUID;

import com.mojang.blaze3d.platform.InputConstants;
import lurch.mc.mixin.NativeImageAccess;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.state.LightmapRenderState;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Mth;
import net.minecraft.world.level.CardinalLighting;
import net.minecraft.world.level.LightLayer;
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
	private static final Path SHM = Path.of("/dev/shm/lurch-mc");
	private static final Identifier OVERLAY = Identifier.fromNamespaceAndPath("lurch", "overlay");
	private static final int HORIZONTAL = 24, BELOW = 8, ABOVE = 24;
	private static final long SCAN_EVERY_NS = 500_000_000L;
	private static final int RESCAN_DISTANCE = 4;

	private static Socket socket;
	private static BufferedReader in;
	private static Writer out;
	private static final StringBuilder events = new StringBuilder();
	private static long lastFrame;
	private static boolean toggleHeld, capturing;
	/** Where lurch last put the player; anything else moving it (a teleport, a respawn) restarts lurch there. */
	private static Vec3 placed;
	private static String lastBlocks = "";
	private static BlockPos scannedAt;
	private static long scannedTime;

	/** The camera lurch asks for; null while lurch is off. */
	public static Vec3 eye;
	public static float pitch, yaw, roll, fov;

	/** The lightmap's inputs as Minecraft last rendered it, and the last light sent. */
	private static float skyFactor, blockFactor, nightVision, darkness, bossDarkening, brightness;
	private static final float[] blockTint = {1, 1, 1}, skyColor = {1, 1, 1}, ambient = new float[3], nightVisionColor = new float[3];
	private static String lastLight = "", lastHotbar = "";
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

	/** The player lurch drives, read by the integrated server's thread; null while lurch is off. */
	private static volatile UUID driven;

	public static boolean on() {
		return socket != null;
	}

	public static boolean drives(UUID player) {
		return player.equals(driven);
	}

	public static boolean moves(Object entity) {
		return on() && entity instanceof LocalPlayer;
	}

	/** Whether input goes to lurch instead of Minecraft: lurch is on and no screen is open. */
	private static boolean ours() {
		Minecraft mc = Minecraft.getInstance();
		return on() && mc.gui.screen() == null && mc.player != null;
	}

	/** True when Minecraft should not see this key. Escape, F1 to F12 and the chat keys stay Minecraft's. */
	public static boolean key(int action, KeyEvent e) {
		Minecraft mc = Minecraft.getInstance();
		int k = e.key();
		if (!ours() || k == InputConstants.KEY_ESCAPE || (k >= InputConstants.KEY_F1 && k <= InputConstants.KEY_F12) || mc.options.keyChat.matches(e) || mc.options.keyCommand.matches(e)) {
			return false;
		}
		if (action != InputConstants.REPEAT) {
			events.append("k ").append(k).append(action == InputConstants.PRESS ? " 1\n" : " 0\n");
		}
		return true;
	}

	public static boolean button(int button, int action) {
		if (!ours()) {
			return false;
		}
		events.append("m ").append(button).append(action == InputConstants.PRESS ? " 1\n" : " 0\n");
		return true;
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
			msg.append(events);
			events.setLength(0);
			msg.append("frame ").append(frametime).append(' ').append(mc.getWindow().getWidth()).append(' ').append(mc.getWindow().getHeight()).append('\n');
			out.write(msg.toString());
			out.flush();
			String line = in.readLine();
			while (line != null && line.startsWith("hud ")) {
				layout(line.split(" "));
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
			float viewPitch = Float.parseFloat(r[11]), viewYaw = Float.parseFloat(r[12]);
			p.setXRot(viewPitch);
			p.setYRot(viewYaw);
			p.xRotO = viewPitch;
			p.yRotO = viewYaw;
			p.setYHeadRot(viewYaw);
			placed = p.position();
			eye = new Vec3(Double.parseDouble(r[4]), Double.parseDouble(r[5]), Double.parseDouble(r[6]));
			pitch = Float.parseFloat(r[7]);
			yaw = Float.parseFloat(r[8]);
			roll = Float.parseFloat(r[9]);
			fov = Float.parseFloat(r[10]);
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

	/** Pushes a pose that draws the hotbar and what sits on it where `hotbar` puts them; false, with nothing pushed, when it puts them nowhere. */
	public static boolean pushHotbar(GuiGraphicsExtractor g) {
		int[] h = hotbar();
		if (h == null) {
			return false;
		}
		float scale = Minecraft.getInstance().getWindow().getGuiScale();
		g.pose().pushMatrix();
		g.pose().translate((h[1] - h[0] * (g.guiWidth() / 2)) / scale, (h[2] - h[0] * g.guiHeight()) / scale);
		g.pose().scale(h[0] / scale);
		return true;
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
		if (!force && now - scannedTime < SCAN_EVERY_NS && at.distManhattan(scannedAt) < RESCAN_DISTANCE) {
			return "";
		}
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
			shm = null;
			capturing = false;
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
		in = null;
		out = null;
		eye = null;
		if (why != null && mc.player != null) {
			mc.player.sendSystemMessage(Component.literal("lurch: " + why));
		}
	}
}
