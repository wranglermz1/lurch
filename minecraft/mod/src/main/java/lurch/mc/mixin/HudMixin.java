package lurch.mc.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import lurch.mc.Lurch;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.Hud;
import net.minecraft.world.entity.player.Player;
import org.joml.Matrix3x2f;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * lurch's viewmodel and HUD under Minecraft's, in place of its crosshair. While lurch is on, the
 * hotbar stays in the middle, sized and placed to sit between Apex's panels, and the armour,
 * hearts, hunger and air move into Apex's player panel.
 */
@Mixin(Hud.class)
public class HudMixin {
	/** The pose before the hotbar's, while the hotbar's is pushed. */
	private final Matrix3x2f lurch$base = new Matrix3x2f();
	private boolean lurch$moved;

	@Inject(method = "extractRenderState", at = @At("HEAD"))
	private void lurch$overlay(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		Lurch.drawOverlay(g);
	}

	@Inject(method = "extractCrosshair", at = @At("HEAD"), cancellable = true)
	private void lurch$crosshair(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		if (Lurch.on()) {
			ci.cancel();
		}
	}

	@Inject(method = "extractHotbarAndDecorations", at = @At("HEAD"))
	private void lurch$hotbar(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		lurch$base.set(g.pose());
		lurch$moved = Lurch.pushHotbar(g);
	}

	@Inject(method = "extractHotbarAndDecorations", at = @At("RETURN"))
	private void lurch$hotbarEnd(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		if (lurch$moved) {
			g.pose().popMatrix();
		}
	}

	@WrapOperation(method = "extractHotbarAndDecorations", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Hud;extractPlayerHealth(Lnet/minecraft/client/gui/GuiGraphicsExtractor;)V"))
	private void lurch$health(Hud hud, GuiGraphicsExtractor g, Operation<Void> op) {
		g.pose().pushMatrix();
		g.pose().set(lurch$base);
		op.call(hud, g);
		g.pose().popMatrix();
	}

	@WrapOperation(method = "extractPlayerHealth", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Hud;extractArmor(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/world/entity/player/Player;IIII)V"))
	private void lurch$armor(GuiGraphicsExtractor g, Player player, int top, int rows, int rowHeight, int left, Operation<Void> op) {
		boolean moved = Lurch.pushPanelRow(g, Lurch.ARMOR, left, top - (rows - 1) * rowHeight - 10);
		op.call(g, player, top, rows, rowHeight, left);
		if (moved) {
			g.pose().popMatrix();
		}
	}

	@WrapOperation(method = "extractPlayerHealth", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Hud;extractHearts(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/world/entity/player/Player;IIIIFIIIZ)V"))
	private void lurch$hearts(Hud hud, GuiGraphicsExtractor g, Player player, int left, int top, int rowHeight, int regen, float maxHealth, int health, int displayHealth, int absorption, boolean blink, Operation<Void> op) {
		boolean moved = Lurch.pushPanelRow(g, Lurch.HEARTS, left, top);
		op.call(hud, g, player, left, top, rowHeight, regen, maxHealth, health, displayHealth, absorption, blink);
		if (moved) {
			g.pose().popMatrix();
		}
	}

	/** Food is drawn right to left from `right`, ten icons 8 apart. */
	@WrapOperation(method = "extractPlayerHealth", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Hud;extractFood(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/world/entity/player/Player;II)V"))
	private void lurch$food(Hud hud, GuiGraphicsExtractor g, Player player, int top, int right, Operation<Void> op) {
		boolean moved = Lurch.pushPanelRow(g, Lurch.FOOD, right - 81, top);
		op.call(hud, g, player, top, right);
		if (moved) {
			g.pose().popMatrix();
		}
	}

	/** The bubbles' line is `getAirBubbleYLine`: up a row for each row of a vehicle's hearts past the first. */
	@WrapOperation(method = "extractPlayerHealth", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Hud;extractAirBubbles(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/world/entity/player/Player;III)V"))
	private void lurch$air(Hud hud, GuiGraphicsExtractor g, Player player, int vehicleHearts, int line, int right, Operation<Void> op) {
		int y = line - ((int) Math.ceil(vehicleHearts / 10.0) - 1) * 10;
		boolean moved = Lurch.pushPanelRow(g, Lurch.AIR, right - 81, y);
		op.call(hud, g, player, vehicleHearts, line, right);
		if (moved) {
			g.pose().popMatrix();
		}
	}
}
