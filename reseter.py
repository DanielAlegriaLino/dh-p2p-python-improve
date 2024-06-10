import subprocess
import threading
import queue
import time
import sys

# Tiempo máximo de espera para la salida (en segundos)
timeout = 3

def monitor_output(process, q):
    """
    Función que monitorea la salida del proceso.
    Si el proceso produce salida, se coloca en la cola 'q'.
    """
    while True:
        line = process.stdout.readline()
        if line:
            q.put(line)
        else:
            break

def execute_script():
    """
    Función que ejecuta el script main.py y lo reinicia si no hay salida en el tiempo especificado.
    """
    while True:
        # Inicia el proceso
        process = subprocess.Popen(['python', 'main.py', '-u admin', '-p DETI2630', '-d' ,'5K08BE5PAZ9F6C1'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        
        # Cola para monitorear la salida
        q = queue.Queue()
        
        # Hilo para leer la salida del proceso
        monitor_thread = threading.Thread(target=monitor_output, args=(process, q))
        monitor_thread.daemon = True  # Permite que el hilo se cierre al terminar el programa principal
        monitor_thread.start()
        
        # Tiempo de espera
        start_time = time.time()
        
        while True:
            try:
                # Intenta obtener una línea de salida con un timeout
                line = q.get(timeout=1)
                print(line, end='')  # Imprime la salida en tiempo real

                # Reinicia el temporizador si hay salida
                start_time = time.time()
            except queue.Empty:
                # Si la cola está vacía, verifica el tiempo de espera
                if time.time() - start_time > timeout:
                    print(f"Sin salida en {timeout} segundos. Reiniciando script...")
                    process.terminate()  # Termina el proceso actual
                    break  # Sale del bucle para reiniciar el proceso
        
        # Asegura que el hilo de monitoreo termine
        monitor_thread.join()
        # Espera a que el proceso termine completamente
        process.wait()

if __name__ == "__main__":
    try:
        execute_script()
    except KeyboardInterrupt:
        print("Ejecución interrumpida por el usuario.")
        sys.exit(0)