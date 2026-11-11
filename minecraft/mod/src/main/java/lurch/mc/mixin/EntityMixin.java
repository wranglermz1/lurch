package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Vanilla's tick still runs for the player lurch moves, but only lurch places it. What the player
 * aims at, the block or entity under the crosshair, is picked from lurch's camera.
 */
@Mixin(Entity.class)
public class EntityMixin {
	@Inject(method = "move", at = @At("HEAD"), cancellable = true)
	private void lurch$move(MoverType type, Vec3 delta, CallbackInfo ci) {
		if (Lurch.moves(this)) {
			ci.cancel();
		}
	}

	@Inject(method = "getEyePosition(F)Lnet/minecraft/world/phys/Vec3;", at = @At("HEAD"), cancellable = true)
	private void lurch$eye(float partialTicks, CallbackInfoReturnable<Vec3> cir) {
		if (Lurch.aims(this)) {
			cir.setReturnValue(Lurch.eye);
		}
	}

	@Inject(method = "getViewVector", at = @At("HEAD"), cancellable = true)
	private void lurch$view(float partialTicks, CallbackInfoReturnable<Vec3> cir) {
		if (Lurch.aims(this)) {
			cir.setReturnValue(Entity.calculateViewVector(Lurch.pitch, Lurch.yaw));
		}
	}
}
