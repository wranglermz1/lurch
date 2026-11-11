package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The first-person camera lurch asks for: its eye, angles with view punch and roll, and FOV. */
@Mixin(Camera.class)
public abstract class CameraMixin {
	@Shadow private boolean detached;
	@Shadow private Entity entity;

	@Shadow protected abstract void setPosition(Vec3 position);

	@Shadow protected abstract void setRotation(float yRot, float xRot);

	private boolean lurch$drives() {
		return Lurch.eye != null && !detached && Lurch.moves(entity);
	}

	@Inject(method = "update", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;alignWithEntity(F)V", shift = At.Shift.AFTER))
	private void lurch$eye(DeltaTracker deltaTracker, CallbackInfo ci) {
		if (lurch$drives()) {
			setRotation(Lurch.yaw, Lurch.pitch);
			setPosition(Lurch.eye);
		}
	}

	@ModifyArg(method = "setRotation", at = @At(value = "INVOKE", target = "Lorg/joml/Quaternionf;rotationYXZ(FFF)Lorg/joml/Quaternionf;"), index = 2)
	private float lurch$roll(float roll) {
		return lurch$drives() ? -Lurch.roll * Mth.DEG_TO_RAD : roll;
	}

	@Inject(method = "calculateFov", at = @At("RETURN"), cancellable = true)
	private void lurch$fov(float partialTicks, CallbackInfoReturnable<Float> cir) {
		if (lurch$drives()) {
			cir.setReturnValue(Lurch.fov);
		}
	}
}
