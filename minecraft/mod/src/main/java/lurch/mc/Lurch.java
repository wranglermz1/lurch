package lurch.mc;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.network.chat.Component;
import net.minecraft.util.Mth;
import net.minecraft.world.phys.Vec3;

/** Hands the local player's movement to lurch-bridge, one usercmd per rendered frame, while F7 has it on. */
public final class Lurch {
	private static final int PORT = 25600;
	private static final int IN_JUMP = 0x2, IN_DUCK = 0x4, IN_FORWARD = 0x8, IN_BACK = 0x10, IN_MOVELEFT = 0x200, IN_MOVERIGHT = 0x400, IN_SPEED = 0x8000;
	private static final int[] BITS = {IN_FORWARD, IN_BACK, IN_MOVELEFT, IN_MOVERIGHT, IN_JUMP, IN_DUCK, IN_SPEED};

	private static Socket socket;
	private static BufferedReader in;
	private static Writer out;
	private static long lastFrame;
	private static boolean toggleHeld;
	/** Where lurch last put the player; anything else moving it (a teleport, a respawn) restarts lurch there. */
	private static Vec3 placed;
	/** Where lurch puts the eye; null while lurch is off. */
	public static Vec3 eye;

	public static boolean moves(Object entity) {
		return socket != null && entity instanceof LocalPlayer;
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
		if (p == null) {
			stop(mc, null);
			return;
		}
		long now = System.nanoTime();
		float frametime = (now - lastFrame) / 1e9f;
		lastFrame = now;
		if (mc.isPaused()) {
			return;
		}
		Options o = mc.options;
		KeyMapping[] keys = {o.keyUp, o.keyDown, o.keyLeft, o.keyRight, o.keyJump, o.keyShift, o.keySprint};
		boolean playing = mc.gui.screen() == null;
		int held = 0, pressed = 0;
		for (int i = 0; i < keys.length; i++) {
			while (keys[i].consumeClick()) {
				pressed |= BITS[i];
			}
			if (keys[i].isDown()) {
				held |= BITS[i];
			}
		}
		if (!playing) {
			held = pressed = 0;
		}
		try {
			if (placed == null || p.position().distanceToSqr(placed) > 1e-4) {
				spawn(p);
			}
			out.write("cmd " + frametime + " " + p.getXRot() + " " + Mth.wrapDegrees(p.getYRot()) + " " + held + " " + pressed + "\n");
			out.flush();
			String line = in.readLine();
			if (line == null) {
				throw new IOException("lurch-bridge closed the connection");
			}
			String[] r = line.split(" ");
			double x = Double.parseDouble(r[1]), y = Double.parseDouble(r[2]), z = Double.parseDouble(r[3]);
			p.setPos(x, y, z);
			p.xo = x;
			p.yo = y;
			p.zo = z;
			p.setDeltaMovement(Vec3.ZERO);
			p.setOnGround(r[7].equals("1"));
			placed = p.position();
			eye = new Vec3(Double.parseDouble(r[4]), Double.parseDouble(r[5]), Double.parseDouble(r[6]));
		} catch (IOException | RuntimeException e) {
			stop(mc, e.toString());
		}
	}

	private static void spawn(LocalPlayer p) throws IOException {
		out.write("spawn " + p.getX() + " " + p.getY() + " " + p.getZ() + " " + Mth.wrapDegrees(p.getYRot()) + "\n");
		placed = p.position();
	}

	private static void start(Minecraft mc) {
		if (mc.player == null) {
			return;
		}
		try {
			socket = new Socket();
			socket.setTcpNoDelay(true);
			socket.setSoTimeout(1000);
			socket.connect(new InetSocketAddress("127.0.0.1", PORT), 1000);
			in = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
			out = new OutputStreamWriter(socket.getOutputStream(), StandardCharsets.US_ASCII);
			placed = null;
			lastFrame = System.nanoTime();
			mc.player.sendSystemMessage(Component.literal("lurch: on"));
		} catch (IOException e) {
			stop(mc, "no lurch-bridge on 127.0.0.1:" + PORT + " (" + e.getMessage() + ")");
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
		in = null;
		out = null;
		eye = null;
		if (why != null && mc.player != null) {
			mc.player.sendSystemMessage(Component.literal("lurch: " + why));
		}
	}
}
