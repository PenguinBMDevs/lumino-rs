@echo off
chcp 65001 >nul
echo ======================================================
echo  协作功能完整测试
echo  服务器: lumino-02.afeu20u3jfocas.dpdns.org:80
echo ======================================================
echo.

echo 正在运行测试...
echo.

cargo test --test collaboration_full_test -- --nocapture 2>&1

echo.
echo ======================================================
echo  测试执行完成
echo ======================================================
pause
